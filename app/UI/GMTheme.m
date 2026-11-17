#import "GMTheme.h"
#import "GMLog.h"
#import <QuartzCore/QuartzCore.h>
#import <objc/runtime.h>

id GMVal(NSDictionary *d, NSString *key) {
    if (![d isKindOfClass:[NSDictionary class]]) return nil;
    id v = [d objectForKey:key];
    return v == [NSNull null] ? nil : v;
}

static UIColor *RGB(int hex) {
    return [UIColor colorWithRed:((hex >> 16) & 255) / 255.0 green:((hex >> 8) & 255) / 255.0 blue:(hex & 255) / 255.0 alpha:1];
}

/// Màu RGB có độ trong (dải màu CoreGraphics cần mọi màu cùng hệ RGB — không dùng colorWithWhite:).
static UIColor *RGBA(int hex, CGFloat a) {
    return [UIColor colorWithRed:((hex >> 16) & 255) / 255.0 green:((hex >> 8) & 255) / 255.0 blue:(hex & 255) / 255.0 alpha:a];
}

/// Chế độ tối đang dùng (chốt lúc `apply` — đổi chế độ thì dựng lại giao diện).
static BOOL gDark;

#pragma mark - Vẽ

static UIImage *GMDraw(CGSize size, void (^draw)(CGContextRef ctx)) {
    UIGraphicsBeginImageContextWithOptions(size, NO, 0);
    draw(UIGraphicsGetCurrentContext());
    UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
    return img;
}

/// Dải màu dọc phủ dải ngang `r` (các màu RGB; `locs` NULL = chia đều).
static void GMVGradient(CGContextRef ctx, CGRect r, NSArray *colors, const CGFloat *locs) {
    CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
    NSMutableArray *cg = [NSMutableArray arrayWithCapacity:colors.count];
    for (UIColor *c in colors) [cg addObject:(__bridge id)c.CGColor];
    CGGradientRef g = CGGradientCreateWithColors(cs, (__bridge CFArrayRef)cg, locs);
    CGContextDrawLinearGradient(ctx, g, CGPointMake(0, CGRectGetMinY(r)), CGPointMake(0, CGRectGetMaxY(r)), 0);
    CGGradientRelease(g);
    CGColorSpaceRelease(cs);
}

/// Dải màu dọc chỉ trong đường `p`.
static void GMFillGradient(CGContextRef ctx, UIBezierPath *p, CGRect r, NSArray *colors, const CGFloat *locs) {
    CGContextSaveGState(ctx);
    [p addClip];
    GMVGradient(ctx, r, colors, locs);
    CGContextRestoreGState(ctx);
}

/// Vùng màu loang tròn: đậm `a` ở tâm, nhạt dần tới 0 ở bán kính `rad`.
static void GMBlob(CGContextRef ctx, CGPoint c, CGFloat rad, int hex, CGFloat a) {
    CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
    NSArray *cols = @[(__bridge id)RGBA(hex, a).CGColor, (__bridge id)RGBA(hex, a * 0.45).CGColor, (__bridge id)RGBA(hex, 0).CGColor];
    CGFloat locs[] = {0, 0.5, 1};
    CGGradientRef g = CGGradientCreateWithColors(cs, (__bridge CFArrayRef)cols, locs);
    CGContextDrawRadialGradient(ctx, g, c, 0, c, rad, 0);
    CGGradientRelease(g);
    CGColorSpaceRelease(cs);
}

static UIImage *GMSolid(UIColor *c, UIColor *line) {
    UIImage *img = GMDraw(CGSizeMake(1, 3), ^(CGContextRef ctx) {
        [c setFill];
        CGContextFillRect(ctx, CGRectMake(0, 0, 1, 3));
        if (line) {
            [line setFill];
            CGContextFillRect(ctx, CGRectMake(0, 2.5, 1, 0.5));
        }
    });
    return [img resizableImageWithCapInsets:UIEdgeInsetsMake(1, 0, 1, 0)];
}

/// Sọc dọc của bảng nhóm iOS 6 (bán trong — màu loang của hình nền ánh qua).
static UIColor *GMStripes(void) {
    static UIColor *cache[2];
    int i = gDark ? 1 : 0;
    if (cache[i]) return cache[i];
    UIColor *a = gDark ? RGBA(0x111A24, 0.8) : RGBA(0xC5CCD3, 0.74);
    UIColor *b = gDark ? RGBA(0x16202B, 0.8) : RGBA(0xCED5DB, 0.74);
    UIImage *t = GMDraw(CGSizeMake(7, 1), ^(CGContextRef ctx) {
        [a setFill];
        CGContextFillRect(ctx, CGRectMake(0, 0, 3, 1));
        [b setFill];
        CGContextFillRect(ctx, CGRectMake(3, 0, 4, 1));
    });
    cache[i] = [UIColor colorWithPatternImage:t];
    return cache[i];
}

/// Nền thanh trên cao `h`: kính sáng bóng (nửa trên sáng hơn, gãy ở giữa như iOS 6), viền sáng mép trên, vạch đáy.
static UIImage *GMNavBg(CGFloat h) {
    BOOL dark = gDark;
    UIImage *img = GMDraw(CGSizeMake(1, h), ^(CGContextRef ctx) {
        NSArray *c = dark ? @[RGBA(0x4E5C6E, 0.84), RGBA(0x3A4656, 0.82), RGBA(0x2A3442, 0.84), RGBA(0x18202B, 0.9)]
                          : @[RGBA(0xFFFFFF, 0.88), RGBA(0xEEF2F7, 0.74), RGBA(0xE2E9F1, 0.68), RGBA(0xD4DEE9, 0.7)];
        CGFloat loc[] = {0, 0.5, 0.5, 1};
        GMVGradient(ctx, CGRectMake(0, 0, 1, h), c, loc);
        [(dark ? RGBA(0xFFFFFF, 0.2) : RGBA(0xFFFFFF, 0.95)) setFill];
        CGContextFillRect(ctx, CGRectMake(0, 0, 1, 1));
        [(dark ? RGBA(0x000000, 0.6) : RGBA(0x3C506A, 0.42)) setFill];
        CGContextFillRect(ctx, CGRectMake(0, h - 1, 1, 1));
    });
    return [img resizableImageWithCapInsets:UIEdgeInsetsZero];
}

/// Nút thanh trên kiểu iOS 6: viền nổi bo góc (hoặc hình mũi tên cho nút quay lại), mặt kính bóng, ấn thì tối lại.
static UIImage *GMBarButtonBg(CGFloat h, BOOL pressed, BOOL back) {
    BOOL dark = gDark;
    const CGFloat r = 5, tip = back ? 11 : 0, w = tip + 2 * r + 3;
    UIImage *img = GMDraw(CGSizeMake(w, h), ^(CGContextRef ctx) {
        CGRect b = CGRectMake(0.5, 0.5, w - 1, h - 2);
        UIBezierPath *(^shape)(CGRect) = ^UIBezierPath *(CGRect f) {
            if (!back) return [UIBezierPath bezierPathWithRoundedRect:f cornerRadius:r];
            CGFloat minX = CGRectGetMinX(f), maxX = CGRectGetMaxX(f), minY = CGRectGetMinY(f), maxY = CGRectGetMaxY(f);
            UIBezierPath *p = [UIBezierPath bezierPath];
            [p moveToPoint:CGPointMake(minX + tip, minY)];
            [p addLineToPoint:CGPointMake(maxX - r, minY)];
            [p addArcWithCenter:CGPointMake(maxX - r, minY + r) radius:r startAngle:-M_PI_2 endAngle:0 clockwise:YES];
            [p addLineToPoint:CGPointMake(maxX, maxY - r)];
            [p addArcWithCenter:CGPointMake(maxX - r, maxY - r) radius:r startAngle:0 endAngle:M_PI_2 clockwise:YES];
            [p addLineToPoint:CGPointMake(minX + tip, maxY)];
            [p addLineToPoint:CGPointMake(minX + 1, CGRectGetMidY(f) + 1)];
            [p addQuadCurveToPoint:CGPointMake(minX + 1, CGRectGetMidY(f) - 1) controlPoint:CGPointMake(minX, CGRectGetMidY(f))];
            [p closePath];
            return p;
        };
        // Viền sáng dưới đáy (nút như khắc vào thanh)
        [(dark ? RGBA(0xFFFFFF, 0.08) : RGBA(0xFFFFFF, 0.55)) setFill];
        [shape(CGRectOffset(b, 0, 1)) fill];
        UIBezierPath *p = shape(b);
        NSArray *c = dark ? (pressed ? @[RGBA(0x354152, 0.95), RGBA(0x1C2530, 0.95)] : @[RGBA(0x5D6D80, 0.9), RGBA(0x2A3644, 0.9)])
                          : (pressed ? @[RGBA(0xC4D1E0, 0.95), RGBA(0xAEBED1, 0.95)] : @[RGBA(0xFFFFFF, 0.9), RGBA(0xE0E8F1, 0.78)]);
        GMFillGradient(ctx, p, b, c, NULL);
        if (!pressed) {
            CGContextSaveGState(ctx);
            [p addClip];
            [(dark ? RGBA(0xFFFFFF, 0.2) : RGBA(0xFFFFFF, 0.95)) setFill];
            CGContextFillRect(ctx, CGRectMake(0, 1.5, w, 1));
            CGContextRestoreGState(ctx);
        }
        [(dark ? RGBA(0x000000, 0.65) : RGBA(0x3C5573, 0.42)) setStroke];
        p.lineWidth = 1;
        [p stroke];
    });
    return [img resizableImageWithCapInsets:UIEdgeInsetsMake(0, tip + r + 1, 0, r + 1)];
}

/// Ô nhập bo tròn lõm: bóng trong ở mép trên, viền sáng dưới đáy. `r` bán kính, `a` độ đục của mặt ô.
static UIImage *GMField(CGFloat r, CGFloat a) {
    BOOL dark = gDark;
    const CGFloat w = 2 * r + 1, h = 2 * r + 2;
    UIImage *img = GMDraw(CGSizeMake(w, h), ^(CGContextRef ctx) {
        CGRect b = CGRectMake(0.5, 0.5, w - 1, h - 2);
        [(dark ? RGBA(0xFFFFFF, 0.08) : RGBA(0xFFFFFF, 0.85)) setFill];
        [[UIBezierPath bezierPathWithRoundedRect:CGRectOffset(b, 0, 1) cornerRadius:r] fill];
        UIBezierPath *p = [UIBezierPath bezierPathWithRoundedRect:b cornerRadius:r];
        [(dark ? RGBA(0x0E1621, MAX(a, 0.88)) : RGBA(0xFFFFFF, a)) setFill];
        [p fill];
        CGContextSaveGState(ctx);
        [p addClip];
        GMVGradient(ctx, CGRectMake(0, 0, w, 4), @[dark ? RGBA(0x000000, 0.55) : RGBA(0x1E3250, 0.22), RGBA(0x000000, 0)], NULL);
        CGContextRestoreGState(ctx);
        [(dark ? RGBA(0x000000, 0.7) : RGBA(0x324664, 0.4)) setStroke];
        p.lineWidth = 1;
        [p stroke];
    });
    return [img resizableImageWithCapInsets:UIEdgeInsetsMake(r, r, r + 1, r)];
}

/// Thanh dưới (thanh mục): kính khói bóng, cả hai chế độ.
static UIImage *GMTabBg(void) {
    UIImage *img = GMDraw(CGSizeMake(1, 49), ^(CGContextRef ctx) {
        NSArray *c = @[RGBA(0x404C5C, 0.86), RGBA(0x2C3744, 0.86), RGBA(0x202934, 0.9), RGBA(0x161D26, 0.93)];
        CGFloat loc[] = {0, 0.5, 0.5, 1};
        GMVGradient(ctx, CGRectMake(0, 0, 1, 49), c, loc);
        [RGBA(0x000000, 0.55) setFill];
        CGContextFillRect(ctx, CGRectMake(0, 0, 1, 1));
        [RGBA(0xFFFFFF, 0.2) setFill];
        CGContextFillRect(ctx, CGRectMake(0, 1, 1, 1));
    });
    return [img resizableImageWithCapInsets:UIEdgeInsetsZero];
}

/// Ô sáng sau mục đang chọn của thanh dưới.
static UIImage *GMTabSelection(void) {
    const CGFloat w = 80, h = 43;
    return GMDraw(CGSizeMake(w, h), ^(CGContextRef ctx) {
        UIBezierPath *p = [UIBezierPath bezierPathWithRoundedRect:CGRectMake(0.5, 3.5, w - 1, h - 5) cornerRadius:6];
        GMFillGradient(ctx, p, CGRectMake(0, 3, w, h - 4), @[RGBA(0xFFFFFF, 0.24), RGBA(0xFFFFFF, 0.06)], NULL);
        CGContextSaveGState(ctx);
        [p addClip];
        [RGBA(0xFFFFFF, 0.3) setFill];
        CGContextFillRect(ctx, CGRectMake(0, 4, w, 1));
        CGContextRestoreGState(ctx);
        [RGBA(0x000000, 0.35) setStroke];
        p.lineWidth = 1;
        [p stroke];
    });
}

#pragma mark - Hình nền

@implementation GMWallpaperView {
    UIImageView *_img;
    UIView *_stripes;
}

- (instancetype)initWithFrame:(CGRect)frame {
    if ((self = [super initWithFrame:frame])) {
        self.clipsToBounds = YES;
        self.userInteractionEnabled = NO;
        self.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
        self.backgroundColor = [GMTheme background];
        _img = [[UIImageView alloc] initWithImage:[GMTheme wallpaper]];
        [self addSubview:_img];
    }
    return self;
}

- (void)setStriped:(BOOL)striped {
    _striped = striped;
    if (striped && !_stripes) {
        _stripes = [[UIView alloc] initWithFrame:self.bounds];
        _stripes.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
        _stripes.backgroundColor = GMStripes();
        [self addSubview:_stripes];
    }
    _stripes.hidden = !striped;
}

- (void)didMoveToWindow {
    [super didMoveToWindow];
    [self setNeedsLayout];
}

/// Canh ảnh theo gốc của màn: vị trí so với view của màn chứa nó (không đổi khi màn đang trượt vào) + chiều cao thanh trên.
- (void)layoutSubviews {
    [super layoutSubviews];
    UIView *v = self;
    UIViewController *vc = nil;
    while (v) {
        id r = v.nextResponder;
        if ([r isKindOfClass:[UIViewController class]]) {
            vc = r;
            break;
        }
        v = v.superview;
    }
    CGPoint o = CGPointZero;
    if (vc && v != self) {
        // Trong hệ toạ độ của view màn (đã xoay theo chiều máy); bảng cuộn: trừ gốc bounds (= vị trí cuộn)
        CGPoint p = [self convertPoint:CGPointZero toView:v];
        o = CGPointMake(p.x - v.bounds.origin.x, p.y - v.bounds.origin.y);
    }
    UINavigationController *nav = vc.navigationController;
    if (nav && !nav.navigationBarHidden && ![vc isKindOfClass:[UINavigationController class]]) {
        o.y += CGRectGetMaxY(nav.navigationBar.frame);
    }
    CGSize s = _img.image.size;
    _img.frame = CGRectMake(-o.x, -o.y, s.width, s.height);
}

@end

@implementation GMTheme

+ (NSInteger)mode {
    return [[NSUserDefaults standardUserDefaults] integerForKey:@"theme.mode"];
}

+ (void)setMode:(NSInteger)mode {
    [[NSUserDefaults standardUserDefaults] setInteger:mode forKey:@"theme.mode"];
    [[NSUserDefaults standardUserDefaults] synchronize];
}

+ (BOOL)wantsDark {
    NSInteger m = [self mode];
    if (m == GMThemeDark) return YES;
    if (m != GMThemeAuto) return NO;
    NSInteger h = [[NSCalendar currentCalendar] components:NSHourCalendarUnit fromDate:[NSDate date]].hour;
    return h >= 19 || h < 6;
}

+ (BOOL)dark {
    return gDark;
}

+ (NSString *)modeText:(NSInteger)m {
    return m == GMThemeDark ? @"Tối" : m == GMThemeAuto ? @"Tự động" : @"Sáng";
}

+ (UIColor *)accent { return gDark ? RGB(0x5EAEF0) : RGB(0x2481CC); }
+ (UIColor *)background { return gDark ? RGB(0x18222D) : RGB(0xEFEFF4); }
+ (UIColor *)chatBackground { return gDark ? RGB(0x0E1621) : RGB(0xD6E2EB); }
+ (UIColor *)secondaryText { return gDark ? RGB(0x8494A4) : RGB(0x6E7B88); }
+ (UIColor *)separator { return gDark ? RGB(0x0F161E) : RGB(0xC8C7CC); }
+ (UIColor *)checkColor { return gDark ? RGB(0x6AB3F3) : RGB(0x3FA34D); }
+ (UIColor *)mutedBadge { return gDark ? RGB(0x4E5A66) : RGB(0xB6B6BB); }
+ (UIColor *)text { return gDark ? RGB(0xF2F4F6) : RGB(0x1C2A38); }
+ (UIColor *)cellBackground { return gDark ? RGB(0x1D2733) : [UIColor whiteColor]; }
+ (UIColor *)barBackground { return gDark ? RGB(0x17212B) : [UIColor colorWithWhite:0.97 alpha:1]; }
+ (UIColor *)fieldBackground { return gDark ? RGB(0x242F3D) : [UIColor whiteColor]; }
+ (UIColor *)placeholderFill { return gDark ? RGB(0x2B3A4A) : [UIColor colorWithWhite:0.88 alpha:1]; }
+ (UIColor *)pillBackground { return gDark ? RGBA(0x14202C, 0.62) : RGBA(0x46607A, 0.5); }
+ (UIColor *)selectedCell { return gDark ? RGB(0x2A3846) : [UIColor colorWithWhite:0.85 alpha:1]; }
+ (UIKeyboardAppearance)keyboard { return gDark ? UIKeyboardAppearanceAlert : UIKeyboardAppearanceDefault; }
+ (UIColor *)glassFill { return gDark ? RGBA(0x1E2C3B, 0.82) : RGBA(0xFFFFFF, 0.68); }
+ (UIColor *)glassGrouped { return gDark ? RGBA(0x1E2C3B, 0.86) : RGBA(0xFFFFFF, 0.76); }
+ (UIColor *)glassLine { return gDark ? RGBA(0x000000, 0.4) : RGBA(0x5A6E87, 0.25); }
+ (UIColor *)barText { return gDark ? RGB(0xE6EDF4) : RGB(0x2F5F93); }
+ (UIColor *)navTitle { return gDark ? RGB(0xF2F4F6) : RGB(0x3A4A5E); }
+ (UIColor *)navTitleShadow { return gDark ? RGBA(0x000000, 0.6) : RGBA(0xFFFFFF, 0.95); }
+ (CGSize)navShadowOffset { return gDark ? CGSizeMake(0, -1) : CGSizeMake(0, 1); }

+ (UIColor *)colorFor:(NSString *)peer {
    static int palette[] = {0xE17076, 0xFAA774, 0xA695E7, 0x7BC862, 0x6EC9CB, 0x65AADD, 0xEE7AAE};
    long long v = llabs(peer.longLongValue);
    return RGB(palette[v % 7]);
}

#pragma mark - Ảnh tự vẽ

/// Hình nền: nền xanh xám, bốn vùng màu loang (cam / xanh / lục / tím nhạt; tối: lam / tím / lục lam), vân chéo mảnh.
+ (UIImage *)wallpaper {
    static UIImage *cache[2];
    int i = gDark ? 1 : 0;
    if (cache[i]) return cache[i];
    CGSize scr = [UIScreen mainScreen].bounds.size;
    CGFloat S = MAX(scr.width, scr.height), W = MIN(scr.width, scr.height), scale = [UIScreen mainScreen].scale;
    BOOL dark = gDark;
    // Vẽ mất ~0,4–0,6 s trên iPad (A4 / A5) → lưu JPEG, lần sau chỉ đọc (đổi cách vẽ thì tăng số «v»)
    NSString *dir = [GMDataDir() stringByAppendingPathComponent:@"ui"];
    NSString *path = [dir stringByAppendingPathComponent:
                              [NSString stringWithFormat:@"wallpaper-v1-%@-%.0f-%.0fx.jpg", dark ? @"dark" : @"light", S, scale]];
    UIImage *saved = [UIImage imageWithData:[NSData dataWithContentsOfFile:path]];
    if (saved.CGImage) {
        cache[i] = [UIImage imageWithCGImage:saved.CGImage scale:scale orientation:UIImageOrientationUp];
        return cache[i];
    }
    long long t0 = (long long)([[NSDate date] timeIntervalSince1970] * 1000);
    cache[i] = GMDraw(CGSizeMake(S, S), ^(CGContextRef ctx) {
        [(dark ? RGB(0x0F1824) : RGB(0xC3D3E1)) setFill];
        CGContextFillRect(ctx, CGRectMake(0, 0, S, S));
        CGFloat R = 0.5 * S;
        if (dark) {
            GMBlob(ctx, CGPointMake(0.2 * W, 0.18 * S), R, 0x3C5FAA, 0.6);
            GMBlob(ctx, CGPointMake(0.86 * W, 0.5 * S), R, 0x7346A0, 0.45);
            GMBlob(ctx, CGPointMake(0.26 * W, 0.86 * S), R, 0x237D7D, 0.42);
            GMBlob(ctx, CGPointMake(0.92 * S, 0.14 * S), R, 0x325096, 0.4);
        } else {
            GMBlob(ctx, CGPointMake(0.18 * W, 0.14 * S), R, 0xFFD696, 0.6);
            GMBlob(ctx, CGPointMake(0.88 * W, 0.42 * S), R, 0x78B2EE, 0.62);
            GMBlob(ctx, CGPointMake(0.25 * W, 0.86 * S), R, 0x96DAB6, 0.58);
            GMBlob(ctx, CGPointMake(0.92 * S, 0.12 * S), R, 0xB4AAEB, 0.42);
        }
        // Vân chéo mảnh (chất liệu kiểu iOS 6)
        CGContextSetLineWidth(ctx, 1);
        [(dark ? RGBA(0xFFFFFF, 0.025) : RGBA(0xFFFFFF, 0.08)) setStroke];
        for (CGFloat k = -S; k < S; k += 4) {
            CGContextMoveToPoint(ctx, k, S);
            CGContextAddLineToPoint(ctx, k + S, 0);
        }
        CGContextStrokePath(ctx);
    });
    GMLog(@"hình nền %.0fx%.0f (%@): vẽ %lld ms", S, S, dark ? @"tối" : @"sáng", (long long)([[NSDate date] timeIntervalSince1970] * 1000) - t0);
    [[NSFileManager defaultManager] createDirectoryAtPath:dir withIntermediateDirectories:YES attributes:nil error:NULL];
    [UIImageJPEGRepresentation(cache[i], 0.92) writeToFile:path atomically:YES];
    return cache[i];
}

static UIImage *GMCircleIcon(UIColor *color, NSString *ini, CGFloat size);

+ (UIImage *)avatarFor:(NSString *)peer name:(NSString *)name size:(CGFloat)size {
    static NSCache *cache;
    if (!cache) cache = [[NSCache alloc] init];
    NSString *key = [NSString stringWithFormat:@"%@|%@|%.0f", peer, name, size];
    UIImage *img = [cache objectForKey:key];
    if (img) return img;
    // Chữ cái đầu của hai từ đầu (bỏ biểu tượng / dấu)
    NSMutableString *ini = [NSMutableString string];
    for (NSString *w in [name componentsSeparatedByCharactersInSet:[NSCharacterSet whitespaceCharacterSet]]) {
        if (!w.length) continue;
        NSRange r = [w rangeOfComposedCharacterSequenceAtIndex:0];
        NSString *c = [w substringWithRange:r];
        if ([c rangeOfCharacterFromSet:[NSCharacterSet alphanumericCharacterSet]].location == NSNotFound) continue;
        [ini appendString:c.uppercaseString];
        if (ini.length >= 2) break;
    }
    if (!ini.length && name.length) [ini appendString:[name substringWithRange:[name rangeOfComposedCharacterSequenceAtIndex:0]]];
    img = GMCircleIcon([self colorFor:peer], ini, size);
    [cache setObject:img forKey:key];
    return img;
}

/// Vòng tròn màu kiểu kính (bóng nửa trên, tối nhẹ phía đáy) + chữ trắng ở giữa — ảnh chữ cái, biểu tượng chủ đề.
static UIImage *GMCircleIcon(UIColor *color, NSString *ini, CGFloat size) {
    return GMDraw(CGSizeMake(size, size), ^(CGContextRef ctx) {
        UIBezierPath *circle = [UIBezierPath bezierPathWithOvalInRect:CGRectMake(0, 0, size, size)];
        [color setFill];
        [circle fill];
        // Bóng kính nửa trên + tối nhẹ phía đáy
        CGContextSaveGState(ctx);
        [circle addClip];
        GMVGradient(ctx, CGRectMake(0, 0, size, size / 2), @[RGBA(0xFFFFFF, 0.5), RGBA(0xFFFFFF, 0.08)], NULL);
        GMVGradient(ctx, CGRectMake(0, size * 0.6, size, size * 0.4), @[RGBA(0x000000, 0), RGBA(0x000000, 0.14)], NULL);
        CGContextRestoreGState(ctx);
        UIFont *f = [UIFont boldSystemFontOfSize:floor(size * 0.38)];
        CGSize ts = [ini sizeWithFont:f];
        CGPoint at = CGPointMake(floor((size - ts.width) / 2), floor((size - ts.height) / 2));
        [RGBA(0x000000, 0.25) set];
        [ini drawAtPoint:CGPointMake(at.x, at.y - 1) withFont:f];
        [[UIColor whiteColor] set];
        [ini drawAtPoint:at withFont:f];
    });
}

+ (UIImage *)topicIcon:(NSString *)title color:(NSInteger)rgb size:(CGFloat)size {
    static NSCache *cache;
    if (!cache) cache = [[NSCache alloc] init];
    NSString *key = [NSString stringWithFormat:@"%@|%ld|%.0f", title, (long)rgb, size];
    UIImage *img = [cache objectForKey:key];
    if (img) return img;
    // Chữ cái đầu của tên (bỏ biểu tượng / dấu ở đầu); không có thì «#»
    NSString *ini = @"#";
    for (NSUInteger i = 0; i < title.length;) {
        NSRange r = [title rangeOfComposedCharacterSequenceAtIndex:i];
        NSString *c = [title substringWithRange:r];
        if ([c rangeOfCharacterFromSet:[NSCharacterSet alphanumericCharacterSet]].location != NSNotFound) {
            ini = c.uppercaseString;
            break;
        }
        i = NSMaxRange(r);
    }
    img = GMCircleIcon(rgb > 0 ? RGB((int)rgb) : RGB(0x8E9AAB), ini, size);
    [cache setObject:img forKey:key];
    return img;
}

+ (CGFloat)tailWidth {
    return 6;
}

/// Bong bóng kính bóng: góc bo 15, đuôi cong ở góc dưới phía người gửi; mặt dải màu, vệt bóng nằm trong phần mũ trên (không bị
/// kéo giãn). Ảnh 2 × bán kính + đuôi, co giãn phần giữa.
+ (UIImage *)bubbleOut:(BOOL)outgoing tail:(BOOL)tail {
    static UIImage *cache[8];
    int idx = (gDark ? 4 : 0) + (outgoing ? 2 : 0) + (tail ? 1 : 0);
    if (cache[idx]) return cache[idx];
    const CGFloat r = 15, t = [self tailWidth], w = 2 * r + t + 2, h = 2 * r + 2;
    NSArray *fill;
    UIColor *stroke, *gloss;
    if (gDark) {
        fill = outgoing ? @[RGBA(0x3A6EA0, 0.95), RGBA(0x244E7A, 0.95)] : @[RGBA(0x22323F, 0.9), RGBA(0x1A2733, 0.9)];
        stroke = outgoing ? RGBA(0x78AFE6, 0.4) : RGBA(0xFFFFFF, 0.1);
        gloss = RGBA(0xFFFFFF, outgoing ? 0.2 : 0.12);
    } else {
        fill = outgoing ? @[RGBA(0xEEFFDA, 0.97), RGBA(0xC8EFA4, 0.95)] : @[RGBA(0xFFFFFF, 0.93), RGBA(0xF2F6FA, 0.88)];
        stroke = outgoing ? RGBA(0x7FB563, 0.8) : RGBA(0x8DA0B4, 0.62);
        gloss = RGBA(0xFFFFFF, 0.7);
    }
    UIImage *img = GMDraw(CGSizeMake(w, h), ^(CGContextRef ctx) {
        // Vẽ cho tin ra (đuôi bên phải); tin vào thì lật ngang
        if (!outgoing) {
            CGContextTranslateCTM(ctx, w, 0);
            CGContextScaleCTM(ctx, -1, 1);
        }
        CGRect b = CGRectMake(0.5, 0.5, w - t - 1, h - 1);
        CGMutablePathRef p = CGPathCreateMutable();
        CGFloat minX = CGRectGetMinX(b), maxX = CGRectGetMaxX(b), minY = CGRectGetMinY(b), maxY = CGRectGetMaxY(b);
        CGPathMoveToPoint(p, NULL, minX + r, minY);
        CGPathAddArcToPoint(p, NULL, maxX, minY, maxX, minY + r, r);
        if (tail) {
            CGPathAddLineToPoint(p, NULL, maxX, maxY - 8);
            CGPathAddQuadCurveToPoint(p, NULL, maxX + 1, maxY - 1, maxX + t + 0.5, maxY);
            CGPathAddQuadCurveToPoint(p, NULL, maxX - 4, maxY + 0.5, maxX - 9, maxY - 3);
            CGPathAddQuadCurveToPoint(p, NULL, maxX - 12, maxY, maxX - r, maxY);
        } else {
            CGPathAddArcToPoint(p, NULL, maxX, maxY, maxX - r, maxY, r);
        }
        CGPathAddArcToPoint(p, NULL, minX, maxY, minX, maxY - r, r);
        CGPathAddArcToPoint(p, NULL, minX, minY, minX + r, minY, r);
        CGPathCloseSubpath(p);
        CGContextSaveGState(ctx);
        CGContextAddPath(ctx, p);
        CGContextClip(ctx);
        GMVGradient(ctx, CGRectMake(0, 0, w, h), fill, NULL);
        GMVGradient(ctx, CGRectMake(0, 1, w, r - 2), @[gloss, RGBA(0xFFFFFF, 0)], NULL);
        CGContextRestoreGState(ctx);
        CGContextAddPath(ctx, p);
        [stroke setStroke];
        CGContextSetLineWidth(ctx, 1);
        CGContextStrokePath(ctx);
        CGPathRelease(p);
    });
    UIEdgeInsets caps = outgoing ? UIEdgeInsetsMake(r, r, r, r + t + 1) : UIEdgeInsetsMake(r, r + t + 1, r, r);
    cache[idx] = [img resizableImageWithCapInsets:caps];
    return cache[idx];
}

+ (UIImage *)glassBar {
    static UIImage *cache[2];
    int i = gDark ? 1 : 0;
    if (cache[i]) return cache[i];
    BOOL dark = gDark;
    UIImage *img = GMDraw(CGSizeMake(1, 48), ^(CGContextRef ctx) {
        GMVGradient(ctx, CGRectMake(0, 0, 1, 48), dark ? @[RGBA(0x343F4E, 0.88), RGBA(0x18202B, 0.92)]
                                                       : @[RGBA(0xFAFCFF, 0.88), RGBA(0xD8E1EB, 0.84)], NULL);
        [(dark ? RGBA(0x000000, 0.6) : RGBA(0x50647D, 0.38)) setFill];
        CGContextFillRect(ctx, CGRectMake(0, 0, 1, 1));
        [(dark ? RGBA(0xFFFFFF, 0.14) : RGBA(0xFFFFFF, 0.95)) setFill];
        CGContextFillRect(ctx, CGRectMake(0, 1, 1, 1));
    });
    cache[i] = [img resizableImageWithCapInsets:UIEdgeInsetsMake(3, 0, 3, 0)];
    return cache[i];
}

+ (UIImage *)fieldImage {
    static UIImage *cache[2];
    int i = gDark ? 1 : 0;
    if (!cache[i]) cache[i] = GMField(16, 0.95);
    return cache[i];
}

+ (UIImage *)sendButtonImage:(BOOL)pressed {
    static UIImage *cache[4];
    int i = (gDark ? 2 : 0) + (pressed ? 1 : 0);
    if (cache[i]) return cache[i];
    BOOL dark = gDark;
    const CGFloat m = 3, r = 7, w = 2 * (m + r) + 1, h = 44;
    UIImage *img = GMDraw(CGSizeMake(w, h), ^(CGContextRef ctx) {
        CGRect b = CGRectMake(m + 0.5, 6.5, w - 2 * m - 1, 31);
        [(dark ? RGBA(0xFFFFFF, 0.08) : RGBA(0xFFFFFF, 0.6)) setFill];
        [[UIBezierPath bezierPathWithRoundedRect:CGRectOffset(b, 0, 1) cornerRadius:r] fill];
        UIBezierPath *p = [UIBezierPath bezierPathWithRoundedRect:b cornerRadius:r];
        NSArray *c = pressed ? @[RGB(0x5A9AD8), RGB(0x2D72BC), RGB(0x1F62A8), RGB(0x174F8C)]
                             : @[RGB(0x7CBCF4), RGB(0x3C8DDC), RGB(0x2A78C8), RGB(0x2068B8)];
        CGFloat loc[] = {0, 0.5, 0.5, 1};
        GMFillGradient(ctx, p, b, c, loc);
        CGContextSaveGState(ctx);
        [p addClip];
        [RGBA(0xFFFFFF, pressed ? 0.2 : 0.5) setFill];
        CGContextFillRect(ctx, CGRectMake(0, 7.5, w, 1));
        CGContextRestoreGState(ctx);
        [(dark ? RGB(0x0D3A6B) : RGB(0x1B5AA0)) setStroke];
        p.lineWidth = 1;
        [p stroke];
    });
    cache[i] = [img resizableImageWithCapInsets:UIEdgeInsetsMake(0, m + r, 0, m + r)];
    return cache[i];
}

+ (UIImage *)cardRow:(BOOL)first last:(BOOL)last selected:(BOOL)sel {
    static NSMutableDictionary *cache;
    if (!cache) cache = [NSMutableDictionary dictionary];
    NSString *key = [NSString stringWithFormat:@"%d%d%d%d", gDark, first, last, sel];
    UIImage *img = [cache objectForKey:key];
    if (img) return img;
    BOOL dark = gDark;
    const CGFloat m = 8, r = 12, W = 2 * m + 2 * r + 1, H = 2 * r + 1;
    UIColor *border = dark ? RGBA(0xFFFFFF, 0.09) : RGBA(0xFFFFFF, 0.8);
    UIColor *fill = sel ? (dark ? RGBA(0x2E4054, 0.94) : RGBA(0xCFE0F3, 0.92)) : [self glassFill];
    img = GMDraw(CGSizeMake(W, H), ^(CGContextRef ctx) {
        UIRectCorner corners = (first ? (UIRectCornerTopLeft | UIRectCornerTopRight) : 0) |
                               (last ? (UIRectCornerBottomLeft | UIRectCornerBottomRight) : 0);
        CGRect outer = CGRectMake(m, 0, W - 2 * m, H);
        [border setFill];
        [[UIBezierPath bezierPathWithRoundedRect:outer byRoundingCorners:corners cornerRadii:CGSizeMake(r, r)]
            fillWithBlendMode:kCGBlendModeCopy alpha:1];
        CGRect in = CGRectMake(m + 1, first ? 1 : 0, W - 2 * m - 2, H - (first ? 1 : 0) - (last ? 1 : 0));
        [fill setFill];
        [[UIBezierPath bezierPathWithRoundedRect:in byRoundingCorners:corners cornerRadii:CGSizeMake(r - 1, r - 1)]
            fillWithBlendMode:kCGBlendModeCopy alpha:1];
        if (first && !sel) {
            [(dark ? RGBA(0xFFFFFF, 0.1) : RGBA(0xFFFFFF, 0.95)) setFill];
            CGContextFillRect(ctx, CGRectMake(m + r, 1, W - 2 * m - 2 * r, 1));
        }
    });
    img = [img resizableImageWithCapInsets:UIEdgeInsetsMake(r, m + r, r, m + r)];
    [cache setObject:img forKey:key];
    return img;
}

+ (UIImage *)badgeImage:(BOOL)muted {
    static UIImage *cache[2];
    int i = muted ? 1 : 0;
    if (cache[i]) return cache[i];
    const CGFloat h = 22, r = 11, w = 2 * r + 1;
    UIImage *img = GMDraw(CGSizeMake(w, h), ^(CGContextRef ctx) {
        UIBezierPath *o = [UIBezierPath bezierPathWithRoundedRect:CGRectMake(0.5, 0.5, w - 1, h - 1) cornerRadius:r - 0.5];
        [RGBA(0x000000, 0.22) setStroke];
        o.lineWidth = 1;
        [o stroke];
        [[UIColor whiteColor] setFill];
        [o fill];
        UIBezierPath *in = [UIBezierPath bezierPathWithRoundedRect:CGRectMake(2, 2, w - 4, h - 4) cornerRadius:r - 2];
        NSArray *c = muted ? @[RGB(0xBAC1C9), RGB(0x9AA3AD), RGB(0x88919B)] : @[RGB(0x5AA6EC), RGB(0x2B78C8), RGB(0x1F66B3)];
        CGFloat loc[] = {0, 0.55, 1};
        GMFillGradient(ctx, in, CGRectMake(0, 2, w, h - 4), c, loc);
        GMFillGradient(ctx, in, CGRectMake(0, 2, w, (h - 4) / 2), @[RGBA(0xFFFFFF, 0.42), RGBA(0xFFFFFF, 0.05)], NULL);
    });
    cache[i] = [img resizableImageWithCapInsets:UIEdgeInsetsMake(0, r, 0, r)];
    return cache[i];
}

/// Biểu tượng thanh dưới 30 × 30: nét mảnh khi không chọn, tô đặc màu sáng khi chọn.
static UIImage *GMTabIcon(NSString *name, BOOL sel) {
    UIColor *c = sel ? RGB(0x6CC2FF) : RGB(0xB9C4D0);
    return GMDraw(CGSizeMake(30, 30), ^(CGContextRef ctx) {
        [c set];
        CGContextSetLineWidth(ctx, 2);
        CGContextSetLineCap(ctx, kCGLineCapRound);
        CGContextSetLineJoin(ctx, kCGLineJoinRound);
        if ([name isEqual:@"library"]) {
            // Nốt nhạc đôi
            UIBezierPath *p = [UIBezierPath bezierPath];
            [p moveToPoint:CGPointMake(11, 22)];
            [p addLineToPoint:CGPointMake(11, 6.5)];
            [p addLineToPoint:CGPointMake(25, 3.5)];
            [p addLineToPoint:CGPointMake(25, 19)];
            p.lineWidth = 2.2;
            [p stroke];
            CGRect h1 = CGRectMake(4, 18.5, 8, 6.5), h2 = CGRectMake(18, 15.5, 8, 6.5);
            if (sel) {
                CGContextFillEllipseInRect(ctx, h1);
                CGContextFillEllipseInRect(ctx, h2);
                CGContextFillRect(ctx, CGRectMake(11, 5.5, 14, 3.5));
            } else {
                CGContextSetLineWidth(ctx, 2);
                CGContextStrokeEllipseInRect(ctx, CGRectInset(h1, 1, 1));
                CGContextStrokeEllipseInRect(ctx, CGRectInset(h2, 1, 1));
            }
        } else if ([name isEqual:@"sources"]) {
            // Sóng phát: chấm giữa + hai vòng cung mỗi bên
            CGRect dot = CGRectMake(12, 12, 6, 6);
            if (sel) CGContextFillEllipseInRect(ctx, dot);
            else CGContextStrokeEllipseInRect(ctx, CGRectInset(dot, 1, 1));
            CGContextSetLineWidth(ctx, sel ? 2.6 : 2);
            for (int k = 0; k < 2; k++) {
                CGFloat r = 7 + k * 5;
                CGContextAddArc(ctx, 15, 15, r, -M_PI_4, M_PI_4, 0);
                CGContextStrokePath(ctx);
                CGContextAddArc(ctx, 15, 15, r, M_PI - M_PI_4, M_PI + M_PI_4, 0);
                CGContextStrokePath(ctx);
            }
            CGContextMoveToPoint(ctx, 15, 18);
            CGContextAddLineToPoint(ctx, 15, 27);
            CGContextStrokePath(ctx);
        } else if ([name isEqual:@"playlist"]) {
            // Ba dòng + nốt nhạc
            CGContextSetLineWidth(ctx, sel ? 2.8 : 2.2);
            for (int k = 0; k < 3; k++) {
                CGFloat y = 6 + k * 6;
                CGContextMoveToPoint(ctx, 3.5, y);
                CGContextAddLineToPoint(ctx, k == 2 ? 13 : 18, y);
            }
            CGContextStrokePath(ctx);
            CGContextMoveToPoint(ctx, 24, 25);
            CGContextAddLineToPoint(ctx, 24, 12);
            CGContextAddLineToPoint(ctx, 28, 13.5);
            CGContextStrokePath(ctx);
            CGRect h = CGRectMake(16.5, 21, 8, 6.5);
            if (sel) CGContextFillEllipseInRect(ctx, h);
            else CGContextStrokeEllipseInRect(ctx, CGRectInset(h, 1, 1));
        } else if ([name isEqual:@"downloads"]) {
            // Mũi tên xuống khay
            UIBezierPath *a = [UIBezierPath bezierPath];
            [a moveToPoint:CGPointMake(15, 3.5)];
            [a addLineToPoint:CGPointMake(15, 18)];
            [a moveToPoint:CGPointMake(8.5, 12)];
            [a addLineToPoint:CGPointMake(15, 18.5)];
            [a addLineToPoint:CGPointMake(21.5, 12)];
            a.lineWidth = sel ? 3 : 2.2;
            a.lineCapStyle = kCGLineCapRound;
            a.lineJoinStyle = kCGLineJoinRound;
            [a stroke];
            UIBezierPath *t = [UIBezierPath bezierPath];
            [t moveToPoint:CGPointMake(4, 19)];
            [t addLineToPoint:CGPointMake(4, 25.5)];
            [t addLineToPoint:CGPointMake(26, 25.5)];
            [t addLineToPoint:CGPointMake(26, 19)];
            t.lineWidth = sel ? 3 : 2.2;
            t.lineJoinStyle = kCGLineJoinRound;
            [t stroke];
        } else {
            // Bánh răng: 8 răng + vành; chọn thì tô đặc chừa lỗ giữa
            CGContextSetLineWidth(ctx, 4);
            for (int k = 0; k < 8; k++) {
                CGFloat a = k * M_PI / 4;
                CGContextMoveToPoint(ctx, 15 + 8 * cos(a), 15 + 8 * sin(a));
                CGContextAddLineToPoint(ctx, 15 + 11.5 * cos(a), 15 + 11.5 * sin(a));
            }
            CGContextStrokePath(ctx);
            if (sel) {
                CGContextFillEllipseInRect(ctx, CGRectMake(5.5, 5.5, 19, 19));
                CGContextSetBlendMode(ctx, kCGBlendModeClear);
                CGContextFillEllipseInRect(ctx, CGRectMake(11, 11, 8, 8));
            } else {
                CGContextSetLineWidth(ctx, 2.2);
                CGContextStrokeEllipseInRect(ctx, CGRectMake(6.5, 6.5, 17, 17));
                CGContextStrokeEllipseInRect(ctx, CGRectMake(11.5, 11.5, 7, 7));
            }
        }
    });
}

+ (UITabBarItem *)tabItem:(NSString *)title icon:(NSString *)icon {
    UITabBarItem *it = [[UITabBarItem alloc] initWithTitle:title image:GMTabIcon(icon, NO) tag:0];
    [it setFinishedSelectedImage:GMTabIcon(icon, YES) withFinishedUnselectedImage:GMTabIcon(icon, NO)];
    return it;
}

+ (UIBarButtonItem *)backItem:(NSString *)title target:(id)target action:(SEL)action {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeCustom];
    [b setBackgroundImage:GMBarButtonBg(30, NO, YES) forState:UIControlStateNormal];
    [b setBackgroundImage:GMBarButtonBg(30, YES, YES) forState:UIControlStateHighlighted];
    [b setTitle:title forState:UIControlStateNormal];
    [b setTitleColor:[self barText] forState:UIControlStateNormal];
    [b setTitleShadowColor:gDark ? RGBA(0x000000, 0.6) : RGBA(0xFFFFFF, 0.9) forState:UIControlStateNormal];
    b.titleLabel.font = [UIFont boldSystemFontOfSize:12];
    b.titleLabel.shadowOffset = CGSizeMake(0, gDark ? -1 : 1);
    b.contentEdgeInsets = UIEdgeInsetsMake(0, 16, 0, 9);
    CGFloat w = [title sizeWithFont:b.titleLabel.font].width + 25;
    b.frame = CGRectMake(0, 0, w, 30);
    [b addTarget:target action:action forControlEvents:UIControlEventTouchUpInside];
    return [[UIBarButtonItem alloc] initWithCustomView:b];
}

+ (UIBarButtonItem *)barButtonIcon:(NSString *)icon target:(id)target action:(SEL)action {
    return [self barButtonImage:[self icon:icon color:[self barText]] target:target action:action];
}

+ (UIBarButtonItem *)barButtonImage:(UIImage *)image target:(id)target action:(SEL)action {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeCustom];
    [b setBackgroundImage:GMBarButtonBg(30, NO, NO) forState:UIControlStateNormal];
    [b setBackgroundImage:GMBarButtonBg(30, YES, NO) forState:UIControlStateHighlighted];
    [b setImage:image forState:UIControlStateNormal];
    b.adjustsImageWhenHighlighted = NO;
    b.frame = CGRectMake(0, 0, 40, 30);
    [b addTarget:target action:action forControlEvents:UIControlEventTouchUpInside];
    return [[UIBarButtonItem alloc] initWithCustomView:b];
}

#pragma mark - Áp giao diện

/// Bảng / ô của app (nguồn dữ liệu là lớp GM…) — bảng của hệ thống (chọn ảnh, xem tệp) giữ nguyên.
static BOOL GMOurTable(UITableView *tv) {
    id ds = tv.dataSource;
    return ds && [NSStringFromClass([ds class]) hasPrefix:@"GM"];
}

static UITableView *GMTableOf(UIView *v) {
    for (UIView *s = v.superview; s; s = s.superview) {
        if ([s isKindOfClass:[UITableView class]]) return (UITableView *)s;
    }
    return nil;
}

/// Bảng: hình nền riêng (bảng nhóm thêm sọc), mặt bảng trong suốt, vạch kẻ kính; bảng thường bỏ vạch của hàng trống.
static void GMStyleTable(UITableView *tv) {
    if (!GMOurTable(tv)) return;
    BOOL grouped = tv.style == UITableViewStyleGrouped;
    tv.backgroundColor = [UIColor clearColor];
    tv.opaque = NO;
    if (tv.tag == GMTagNoWallpaper) {
        tv.backgroundView = nil;
    } else {
        GMWallpaperView *wv = (GMWallpaperView *)tv.backgroundView;
        if (![wv isKindOfClass:[GMWallpaperView class]]) {
            wv = [[GMWallpaperView alloc] initWithFrame:tv.bounds];
            tv.backgroundView = wv;
        }
        wv.striped = grouped;
    }
    if (tv.separatorStyle != UITableViewCellSeparatorStyleNone) {
        tv.separatorColor = grouped ? (gDark ? RGBA(0x000000, 0.55) : RGBA(0x6E7D8C, 0.5)) : [GMTheme glassLine];
        if (gDark && grouped) tv.separatorStyle = UITableViewCellSeparatorStyleSingleLine;  // bỏ vạch khắc trắng
    }
    if (!grouped && !tv.tableFooterView) tv.tableFooterView = [[UIView alloc] initWithFrame:CGRectZero];
}

/// Chế độ tối: chữ tiêu đề / chú thích nhóm của bảng nhóm (mặc định xám đậm, bóng trắng — chìm trên nền tối).
static void GMRecolorHeader(UIView *v) {
    if ([v isKindOfClass:[UILabel class]]) {
        UILabel *l = (UILabel *)v;
        if (l.tag == 4243) return;
        l.tag = 4243;
        l.textColor = [GMTheme secondaryText];
        l.shadowColor = RGBA(0x000000, 0.5);
        l.shadowOffset = CGSizeMake(0, -1);
        return;
    }
    for (UIView *s in v.subviews) GMRecolorHeader(s);
}

static void GMStyleHeaders(UITableView *tv) {
    if (!gDark || tv.style != UITableViewStyleGrouped || !GMOurTable(tv)) return;
    for (UIView *v in tv.subviews) {
        if ([v isKindOfClass:[UITableViewCell class]] || v == tv.backgroundView || v == tv.tableHeaderView ||
            v == tv.tableFooterView || [v isKindOfClass:[UIImageView class]]) continue;
        GMRecolorHeader(v);
    }
}

/// Màu tối của chữ mặc định (đen / xám / xanh xám của iOS) — chữ đã đặt màu riêng (xanh chủ đạo, đỏ, trắng) giữ nguyên.
static BOOL GMIsDefaultDark(UIColor *c) {
    CGFloat r = 0, g = 0, b = 0, a = 0;
    if (![c getRed:&r green:&g blue:&b alpha:&a]) {
        CGFloat w = 0;
        if (![c getWhite:&w alpha:&a]) return NO;
        r = g = b = w;
    }
    CGFloat mx = MAX(r, MAX(g, b)), mn = MIN(r, MIN(g, b));
    return mx < 0.6 && (mx - mn < 0.35);
}

static Class gBubbleCell;

static BOOL GMSkipCell(UITableViewCell *c) {
    if (c.tag == GMTagOwnBackground || [c isKindOfClass:gBubbleCell]) return YES;
    UITableView *tv = GMTableOf(c);
    return !tv || !GMOurTable(tv);
}

/// Chữ trong ô nằm trên kính: nền chữ trong suốt (UIKit hay đặt nền chữ = nền ô → hai lớp kính chồng thành mảng trắng).
static void GMClearLabels(UITableViewCell *c) {
    if (GMSkipCell(c)) return;
    if (c.textLabel) c.textLabel.backgroundColor = [UIColor clearColor];
    if (c.detailTextLabel) c.detailTextLabel.backgroundColor = [UIColor clearColor];
}

static void GMStyleCell(UITableViewCell *c) {
    if (GMSkipCell(c)) return;
    BOOL grouped = GMTableOf(c).style == UITableViewStyleGrouped;
    UIColor *fill = grouped ? [GMTheme glassGrouped] : [GMTheme glassFill];
    if (![c.backgroundColor isEqual:fill]) c.backgroundColor = fill;
    // Ô kiểu Default không có detailTextLabel (nil — không được cho vào mảng)
    NSMutableArray *labels = [NSMutableArray array];
    if (c.textLabel) [labels addObject:c.textLabel];
    if (c.detailTextLabel) [labels addObject:c.detailTextLabel];
    for (UILabel *l in labels) {
        l.backgroundColor = [UIColor clearColor];
        if (gDark && l.text.length && GMIsDefaultDark(l.textColor)) {
            l.textColor = l == c.textLabel ? [GMTheme text] : [GMTheme secondaryText];
        }
    }
    if (gDark && (!c.selectedBackgroundView || c.selectedBackgroundView.tag != 4242)) {
        UIView *sel = [[UIView alloc] init];
        sel.tag = 4242;
        sel.backgroundColor = [GMTheme selectedCell];
        c.selectedBackgroundView = sel;
    }
}

static void GMSwizzle(Class cls, SEL sel, void (^after)(id me)) {
    Method m = class_getInstanceMethod(cls, sel);
    if (!m) return;
    void (*orig)(id, SEL) = (void (*)(id, SEL))method_getImplementation(m);
    method_setImplementation(m, imp_implementationWithBlock(^(id me) {
        orig(me, sel);
        after(me);
    }));
}

/// Như GMSwizzle cho hàm hai đối số BOOL (setHighlighted:animated:, setSelected:animated:).
static void GMSwizzleBB(Class cls, SEL sel, void (^after)(id me)) {
    Method m = class_getInstanceMethod(cls, sel);
    if (!m) return;
    void (*orig)(id, SEL, BOOL, BOOL) = (void (*)(id, SEL, BOOL, BOOL))method_getImplementation(m);
    method_setImplementation(m, imp_implementationWithBlock(^(id me, BOOL a, BOOL b) {
        orig(me, sel, a, b);
        after(me);
    }));
}

+ (void)apply {
    gDark = [self wantsDark];
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        gBubbleCell = NSClassFromString(@"GMBubbleCell");
        GMSwizzle([UITableView class], @selector(didMoveToWindow), ^(id me) { GMStyleTable(me); });
        GMSwizzle([UITableView class], @selector(layoutSubviews), ^(id me) { GMStyleHeaders(me); });
        GMSwizzle([UITableViewCell class], @selector(layoutSubviews), ^(id me) { GMStyleCell(me); });
        GMSwizzleBB([UITableViewCell class], @selector(setHighlighted:animated:), ^(id me) { GMClearLabels(me); });
        GMSwizzleBB([UITableViewCell class], @selector(setSelected:animated:), ^(id me) { GMClearLabels(me); });
    });
    [UIApplication sharedApplication].statusBarStyle = gDark ? UIStatusBarStyleBlackOpaque : UIStatusBarStyleDefault;
    NSValue *shadowOff = [NSValue valueWithUIOffset:UIOffsetMake(0, gDark ? -1 : 1)];

    // Thanh trên: kính bóng, tiêu đề nổi
    UINavigationBar *nb = [UINavigationBar appearance];
    [nb setBackgroundImage:GMNavBg(44) forBarMetrics:UIBarMetricsDefault];
    [nb setBackgroundImage:GMNavBg(32) forBarMetrics:UIBarMetricsLandscapePhone];
    [nb setTitleTextAttributes:@{
        UITextAttributeTextColor: [self navTitle],
        UITextAttributeTextShadowColor: [self navTitleShadow],
        UITextAttributeTextShadowOffset: shadowOff,
        UITextAttributeFont: [UIFont boldSystemFontOfSize:20],
    }];

    // Nút thanh trên + nút «Huỷ» của ô tìm: viền nổi kiểu iOS 6, nút quay lại hình mũi tên
    NSDictionary *normal = @{
        UITextAttributeTextColor: [self barText],
        UITextAttributeTextShadowColor: gDark ? RGBA(0x000000, 0.6) : RGBA(0xFFFFFF, 0.9),
        UITextAttributeTextShadowOffset: shadowOff,
        UITextAttributeFont: [UIFont boldSystemFontOfSize:12],
    };
    NSMutableDictionary *disabled = [normal mutableCopy];
    [disabled setObject:[[self barText] colorWithAlphaComponent:0.4] forKey:UITextAttributeTextColor];
    for (UIBarButtonItem *bi in @[[UIBarButtonItem appearanceWhenContainedIn:[UINavigationBar class], nil],
                                  [UIBarButtonItem appearanceWhenContainedIn:[UISearchBar class], nil]]) {
        for (NSNumber *m in @[@(UIBarMetricsDefault), @(UIBarMetricsLandscapePhone)]) {
            CGFloat h = m.integerValue == UIBarMetricsDefault ? 30 : 24;
            [bi setBackgroundImage:GMBarButtonBg(h, NO, NO) forState:UIControlStateNormal barMetrics:m.integerValue];
            [bi setBackgroundImage:GMBarButtonBg(h, YES, NO) forState:UIControlStateHighlighted barMetrics:m.integerValue];
            [bi setBackButtonBackgroundImage:GMBarButtonBg(h, NO, YES) forState:UIControlStateNormal barMetrics:m.integerValue];
            [bi setBackButtonBackgroundImage:GMBarButtonBg(h, YES, YES) forState:UIControlStateHighlighted barMetrics:m.integerValue];
            [bi setBackButtonTitlePositionAdjustment:UIOffsetMake(1, 0) forBarMetrics:m.integerValue];
        }
        [bi setTitleTextAttributes:normal forState:UIControlStateNormal];
        [bi setTitleTextAttributes:normal forState:UIControlStateHighlighted];
        [bi setTitleTextAttributes:disabled forState:UIControlStateDisabled];
    }

    // Ô tìm: nền trong suốt (lộ hình nền), ô nhập bo tròn lõm
    UISearchBar *sb = [UISearchBar appearance];
    [sb setBackgroundImage:GMSolid([UIColor clearColor], nil)];
    [sb setSearchFieldBackgroundImage:GMField(15, 0.8) forState:UIControlStateNormal];

    // Thanh dưới: kính khói, ô sáng sau mục đang chọn, chữ sáng
    UITabBar *tb = [UITabBar appearance];
    [tb setBackgroundImage:GMTabBg()];
    [tb setSelectionIndicatorImage:GMTabSelection()];
    NSDictionary *tabNormal = @{
        UITextAttributeTextColor: RGB(0xB9C4D0),
        UITextAttributeTextShadowColor: RGBA(0x000000, 0.5),
        UITextAttributeTextShadowOffset: [NSValue valueWithUIOffset:UIOffsetMake(0, -1)],
        UITextAttributeFont: [UIFont boldSystemFontOfSize:10],
    };
    NSMutableDictionary *tabSel = [tabNormal mutableCopy];
    [tabSel setObject:[UIColor whiteColor] forKey:UITextAttributeTextColor];
    UITabBarItem *ti = [UITabBarItem appearance];
    [ti setTitleTextAttributes:tabNormal forState:UIControlStateNormal];
    [ti setTitleTextAttributes:tabSel forState:UIControlStateSelected];
    [ti setTitleTextAttributes:tabSel forState:UIControlStateHighlighted];
}

#pragma mark - Giờ, ngày

static NSDateFormatter *GMFormatter(NSString *fmt) {
    static NSMutableDictionary *cache;
    if (!cache) cache = [NSMutableDictionary dictionary];
    NSDateFormatter *f = [cache objectForKey:fmt];
    if (!f) {
        f = [[NSDateFormatter alloc] init];
        f.dateFormat = fmt;
        [cache setObject:f forKey:fmt];
    }
    return f;
}

static NSDateComponents *GMDay(NSDate *d) {
    return [[NSCalendar currentCalendar] components:NSYearCalendarUnit | NSMonthCalendarUnit | NSDayCalendarUnit | NSWeekdayCalendarUnit
                                           fromDate:d];
}

+ (BOOL)sameDay:(NSNumber *)a as:(NSNumber *)b {
    if (!a || !b) return NO;
    NSDateComponents *x = GMDay([NSDate dateWithTimeIntervalSince1970:a.doubleValue]);
    NSDateComponents *y = GMDay([NSDate dateWithTimeIntervalSince1970:b.doubleValue]);
    return x.year == y.year && x.month == y.month && x.day == y.day;
}

+ (NSString *)clock:(NSNumber *)unix {
    if (!unix) return @"";
    return [GMFormatter(@"HH:mm") stringFromDate:[NSDate dateWithTimeIntervalSince1970:unix.doubleValue]];
}

+ (NSString *)shortTime:(NSNumber *)unix {
    if (!unix) return @"";
    NSDate *d = [NSDate dateWithTimeIntervalSince1970:unix.doubleValue];
    NSNumber *now = @([[NSDate date] timeIntervalSince1970]);
    if ([self sameDay:unix as:now]) return [GMFormatter(@"HH:mm") stringFromDate:d];
    NSTimeInterval ago = now.doubleValue - unix.doubleValue;
    if (ago < 6 * 86400) {
        static NSString *days[] = {@"", @"CN", @"T2", @"T3", @"T4", @"T5", @"T6", @"T7"};
        return days[GMDay(d).weekday];
    }
    if (GMDay(d).year == GMDay([NSDate date]).year) return [GMFormatter(@"dd/MM") stringFromDate:d];
    return [GMFormatter(@"dd/MM/yy") stringFromDate:d];
}

+ (NSString *)dayTitle:(NSNumber *)unix {
    NSDate *d = [NSDate dateWithTimeIntervalSince1970:unix.doubleValue];
    NSNumber *now = @([[NSDate date] timeIntervalSince1970]);
    if ([self sameDay:unix as:now]) return @"Hôm nay";
    if ([self sameDay:unix as:@(now.doubleValue - 86400)]) return @"Hôm qua";
    NSDateComponents *c = GMDay(d);
    if (c.year == GMDay([NSDate date]).year) return [NSString stringWithFormat:@"%ld tháng %ld", (long)c.day, (long)c.month];
    return [NSString stringWithFormat:@"%ld tháng %ld, %ld", (long)c.day, (long)c.month, (long)c.year];
}

#pragma mark - Khác

+ (NSString *)preview:(NSDictionary *)msg {
    if (!msg) return @"";
    NSString *text = GMVal(msg, @"text") ?: @"";
    if ([GMVal(msg, @"service") boolValue]) return text.length ? text : @"Tin hệ thống";
    NSString *media = GMVal(msg, @"media");
    NSString *dk = GMVal(GMVal(msg, @"doc"), @"kind");
    if ([dk isEqual:@"gif"]) media = @"gif";
    else if ([dk isEqual:@"round"]) media = @"round";
    if (!media) return text;
    NSDictionary *labels = @{
        @"photo": @"Ảnh", @"video": @"Video", @"voice": @"Tin thoại", @"audio": @"Âm thanh",
        @"sticker": @"Sticker", @"file": @"Tệp", @"image": @"Ảnh", @"contact": @"Danh bạ",
        @"poll": @"Bình chọn", @"geo": @"Vị trí", @"dice": @"Xúc xắc", @"gif": @"GIF", @"round": @"Video tròn",
    };
    NSString *label = [labels objectForKey:media] ?: @"Media";
    NSString *fname = GMVal(GMVal(msg, @"doc"), @"name");
    if ([media isEqual:@"file"] && fname.length) label = [@"📄 " stringByAppendingString:fname];
    return text.length ? [NSString stringWithFormat:@"%@, %@", label, text] : label;
}

+ (void)toast:(NSString *)text in:(UIView *)view {
    UILabel *l = [[UILabel alloc] init];
    l.text = text;
    l.font = [UIFont boldSystemFontOfSize:14];
    l.textColor = [UIColor whiteColor];
    l.textAlignment = GMAlignCenter;
    l.backgroundColor = [UIColor colorWithWhite:0 alpha:0.75];
    l.layer.cornerRadius = 10;
    l.clipsToBounds = YES;
    CGSize s = [text sizeWithFont:l.font];
    CGFloat w = MIN(view.bounds.size.width - 40, s.width + 30);
    l.frame = CGRectMake(floor((view.bounds.size.width - w) / 2), floor(view.bounds.size.height * 0.62), w, 36);
    l.autoresizingMask = UIViewAutoresizingFlexibleLeftMargin | UIViewAutoresizingFlexibleRightMargin;
    [view addSubview:l];
    [UIView animateWithDuration:0.3 delay:1.6 options:0 animations:^{ l.alpha = 0; }
                     completion:^(BOOL finished) { [l removeFromSuperview]; }];
}

+ (UIImage *)icon:(NSString *)name {
    return [self icon:name color:[self accent]];
}

+ (UIImage *)icon:(NSString *)name color:(UIColor *)c {
    static NSMutableDictionary *cache;
    if (!cache) cache = [NSMutableDictionary dictionary];
    NSString *ckey = [NSString stringWithFormat:@"%@|%@", name, c];
    UIImage *img = [cache objectForKey:ckey];
    if (img) return img;
    img = GMDraw(CGSizeMake(26, 26), ^(CGContextRef ctx) {
        [c set];
        CGContextSetLineWidth(ctx, 1.8);
        CGContextSetLineCap(ctx, kCGLineCapRound);
        if ([name isEqual:@"group"]) {
            // Nhóm: hai đầu người
            CGContextSetLineWidth(ctx, 1.8);
            CGContextStrokeEllipseInRect(ctx, CGRectMake(5, 5, 7, 7));
            CGContextAddArc(ctx, 8.5, 21, 6, M_PI, 0, 0);
            CGContextStrokePath(ctx);
            CGContextStrokeEllipseInRect(ctx, CGRectMake(14.5, 7, 6, 6));
            CGContextAddArc(ctx, 17.5, 21.5, 5, M_PI * 1.15, 0, 0);
            CGContextStrokePath(ctx);
        } else if ([name isEqual:@"sticker"]) {
            // Mặt cười: vòng tròn + hai mắt + miệng
            CGContextSetLineWidth(ctx, 1.8);
            CGContextStrokeEllipseInRect(ctx, CGRectMake(3.5, 3.5, 19, 19));
            CGContextFillEllipseInRect(ctx, CGRectMake(9, 9, 2.4, 3));
            CGContextFillEllipseInRect(ctx, CGRectMake(14.6, 9, 2.4, 3));
            CGContextAddArc(ctx, 13, 13.5, 5, 0.35, M_PI - 0.35, 0);
            CGContextStrokePath(ctx);
        } else if ([name isEqual:@"keyboard"]) {
            // Bàn phím: khung bo + các phím nhỏ
            CGContextSetLineWidth(ctx, 1.6);
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(2.5, 6.5, 21, 14) cornerRadius:2.5] stroke];
            for (int r = 0; r < 2; r++) {
                for (int k = 0; k < 5; k++) CGContextFillRect(ctx, CGRectMake(5 + k * 3.6, 9.5 + r * 3.3, 2.2, 2));
            }
            CGContextFillRect(ctx, CGRectMake(8, 16.4, 10, 2));
        } else if ([name isEqual:@"compose"]) {
            // Soạn tin mới: tờ giấy (hở góc trên phải) + bút chì chéo — nét đậm, màu chủ đạo (thanh trắng phẳng)
            CGContextSetLineWidth(ctx, 2);
            CGContextSetLineJoin(ctx, kCGLineJoinRound);
            CGContextMoveToPoint(ctx, 13, 4.5);
            CGContextAddLineToPoint(ctx, 6.5, 4.5);
            CGContextAddQuadCurveToPoint(ctx, 4, 4.5, 4, 7);
            CGContextAddLineToPoint(ctx, 4, 19.5);
            CGContextAddQuadCurveToPoint(ctx, 4, 22, 6.5, 22);
            CGContextAddLineToPoint(ctx, 19, 22);
            CGContextAddQuadCurveToPoint(ctx, 21.5, 22, 21.5, 19.5);
            CGContextAddLineToPoint(ctx, 21.5, 13);
            CGContextStrokePath(ctx);
            CGContextSetLineWidth(ctx, 3.4);
            CGContextMoveToPoint(ctx, 13.2, 12.8);
            CGContextAddLineToPoint(ctx, 21.5, 4.5);
            CGContextStrokePath(ctx);
            CGContextMoveToPoint(ctx, 10.2, 15.8);  // ngòi
            CGContextAddLineToPoint(ctx, 11.4, 12.3);
            CGContextAddLineToPoint(ctx, 13.7, 14.6);
            CGContextClosePath(ctx);
            CGContextFillPath(ctx);
        } else if ([name isEqual:@"refresh"]) {
            // Làm mới: vòng gần kín + đầu mũi tên
            CGContextSetLineWidth(ctx, 2.2);
            CGContextAddArc(ctx, 13, 13.5, 8, -M_PI / 3, M_PI * 1.45, 0);
            CGContextStrokePath(ctx);
            CGFloat ex = 13 + 8 * cos(-M_PI / 3), ey = 13.5 + 8 * sin(-M_PI / 3);
            CGContextMoveToPoint(ctx, ex + 1.5, ey - 5);
            CGContextAddLineToPoint(ctx, ex + 1.8, ey + 1.2);
            CGContextAddLineToPoint(ctx, ex - 4.2, ey + 0.4);
            CGContextClosePath(ctx);
            CGContextFillPath(ctx);
        } else if ([name isEqual:@"mic"]) {
            UIBezierPath *cap = [UIBezierPath bezierPathWithRoundedRect:CGRectMake(9, 3, 8, 13) cornerRadius:4];
            [cap fill];
            CGContextAddArc(ctx, 13, 11, 7, 0, M_PI, 0);
            CGContextStrokePath(ctx);
            CGContextMoveToPoint(ctx, 13, 18);
            CGContextAddLineToPoint(ctx, 13, 22);
            CGContextMoveToPoint(ctx, 9, 22.5);
            CGContextAddLineToPoint(ctx, 17, 22.5);
            CGContextStrokePath(ctx);
        } else {
            // Kẹp giấy: hai vòng lồng nhau
            CGContextMoveToPoint(ctx, 16, 7);
            CGContextAddLineToPoint(ctx, 16, 18);
            CGContextAddArc(ctx, 12.5, 18, 3.5, 0, M_PI, 0);
            CGContextAddLineToPoint(ctx, 9, 5.5);
            CGContextAddArc(ctx, 11.5, 5.5, 2.5, M_PI, 0, 0);
            CGContextAddLineToPoint(ctx, 14, 17);
            CGContextAddArc(ctx, 12.5, 17, 1.5, 0, M_PI, 0);
            CGContextAddLineToPoint(ctx, 11, 8);
            CGContextStrokePath(ctx);
        }
    });
    [cache setObject:img forKey:ckey];
    return img;
}

+ (UIImage *)roundImage:(UIImage *)img size:(CGFloat)size {
    return GMDraw(CGSizeMake(size, size), ^(CGContextRef ctx) {
        [[UIBezierPath bezierPathWithOvalInRect:CGRectMake(0, 0, size, size)] addClip];
        [img drawInRect:CGRectMake(0, 0, size, size)];
    });
}

+ (UIImage *)settingsIcon:(NSString *)kind {
    static NSMutableDictionary *cache;
    if (!cache) cache = [NSMutableDictionary dictionary];
    UIImage *img = [cache objectForKey:kind];
    if (img) return img;
    // Hai sắc mỗi biểu tượng (trên sáng → dưới đậm), như biểu tượng Cài đặt của iOS 6
    int top, bot;
    if ([kind isEqual:@"saved"]) top = 0xA596EC, bot = 0x6E5BD0;
    else if ([kind isEqual:@"proxy"]) top = 0x54C6D8, bot = 0x1D8EA3;
    else if ([kind isEqual:@"storage"]) top = 0x6FD27A, bot = 0x2FA43F;
    else if ([kind isEqual:@"notify"]) top = 0xFF7B72, bot = 0xE0352B;
    else if ([kind isEqual:@"lock"]) top = 0x8B89F0, bot = 0x4F4CC9;
    else if ([kind isEqual:@"theme"]) top = 0x5D7590, bot = 0x2C3E52;
    else if ([kind isEqual:@"stickers"]) top = 0xFFC25A, bot = 0xF08A1C;
    else if ([kind isEqual:@"folders"]) top = 0x6FB8F5, bot = 0x2C7FD6;
    else if ([kind isEqual:@"community"]) top = 0x5FD0F0, bot = 0x1E96C8;
    else if ([kind isEqual:@"ahead"]) top = 0x4CD7B0, bot = 0x16A383;
    else if ([kind isEqual:@"lyrics"]) top = 0xFF83B0, bot = 0xE0457E;
    else if ([kind isEqual:@"music"]) top = 0xFFA34D, bot = 0xF0641E;
    else top = 0xA9B2BC, bot = 0x76808B;
    img = GMDraw(CGSizeMake(29, 29), ^(CGContextRef ctx) {
        UIBezierPath *sq = [UIBezierPath bezierPathWithRoundedRect:CGRectMake(0.5, 0.5, 28, 28) cornerRadius:6.5];
        GMFillGradient(ctx, sq, CGRectMake(0, 0, 29, 29), @[RGB(top), RGB(bot)], NULL);
        [RGBA(0x000000, 0.22) setStroke];
        sq.lineWidth = 1;
        [sq stroke];
        [[UIColor whiteColor] set];
        if ([kind isEqual:@"saved"]) {
            // Dấu trang: chữ nhật có khía chữ V ở đáy
            CGContextMoveToPoint(ctx, 9.5, 6.5);
            CGContextAddLineToPoint(ctx, 19.5, 6.5);
            CGContextAddLineToPoint(ctx, 19.5, 23);
            CGContextAddLineToPoint(ctx, 14.5, 18.5);
            CGContextAddLineToPoint(ctx, 9.5, 23);
            CGContextClosePath(ctx);
            CGContextFillPath(ctx);
        } else if ([kind isEqual:@"proxy"]) {
            // Khiên
            CGContextMoveToPoint(ctx, 14.5, 5.5);
            CGContextAddLineToPoint(ctx, 22, 8.5);
            CGContextAddCurveToPoint(ctx, 22, 16, 19, 21, 14.5, 23.5);
            CGContextAddCurveToPoint(ctx, 10, 21, 7, 16, 7, 8.5);
            CGContextClosePath(ctx);
            CGContextFillPath(ctx);
        } else if ([kind isEqual:@"theme"]) {
            // Trăng khuyết: tròn trắng trừ đi một tròn lệch (tô chẵn-lẻ trong vùng tròn đầu)
            CGContextSaveGState(ctx);
            CGContextAddEllipseInRect(ctx, CGRectMake(6.5, 6.5, 16, 16));
            CGContextClip(ctx);
            CGContextAddRect(ctx, CGRectMake(0, 0, 29, 29));
            CGContextAddEllipseInRect(ctx, CGRectMake(11.5, 3.5, 14, 14));
            CGContextEOFillPath(ctx);
            CGContextRestoreGState(ctx);
        } else if ([kind isEqual:@"community"]) {
            // Hai bong bóng chat chồng nhau (nhóm trò chuyện)
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(5, 6, 13, 10) cornerRadius:3] fill];
            CGContextMoveToPoint(ctx, 8, 15.5);
            CGContextAddLineToPoint(ctx, 7, 19);
            CGContextAddLineToPoint(ctx, 11.5, 15.5);
            CGContextFillPath(ctx);
            [[UIColor colorWithWhite:1 alpha:0.75] setFill];
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(11, 11.5, 13, 10) cornerRadius:3] fill];
            CGContextMoveToPoint(ctx, 21, 21);
            CGContextAddLineToPoint(ctx, 22, 24.5);
            CGContextAddLineToPoint(ctx, 17.5, 21);
            CGContextFillPath(ctx);
            [[UIColor whiteColor] set];
        } else if ([kind isEqual:@"folders"]) {
            // Thư mục: tai + thân
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(6, 8, 8, 4) cornerRadius:1.5] fill];
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(6, 10, 17, 12) cornerRadius:2] fill];
        } else if ([kind isEqual:@"stickers"]) {
            // Mặt cười: tròn trắng, mắt + miệng màu cam đậm
            CGContextFillEllipseInRect(ctx, CGRectMake(6, 6, 17, 17));
            [RGB(0xD9741A) set];
            CGContextFillEllipseInRect(ctx, CGRectMake(10.2, 10.5, 2.4, 3));
            CGContextFillEllipseInRect(ctx, CGRectMake(16.4, 10.5, 2.4, 3));
            CGContextSetLineWidth(ctx, 1.6);
            CGContextSetLineCap(ctx, kCGLineCapRound);
            CGContextAddArc(ctx, 14.5, 14.5, 4.6, 0.45, M_PI - 0.45, 0);
            CGContextStrokePath(ctx);
            [[UIColor whiteColor] set];
        } else if ([kind isEqual:@"lock"]) {
            // Ổ khoá: quai + thân
            CGContextSetLineWidth(ctx, 2.2);
            CGContextAddArc(ctx, 14.5, 12, 4.5, M_PI, 0, 0);
            CGContextAddLineToPoint(ctx, 19, 14);
            CGContextMoveToPoint(ctx, 10, 12);
            CGContextAddLineToPoint(ctx, 10, 14);
            CGContextStrokePath(ctx);
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(7.5, 13.5, 14, 10) cornerRadius:2] fill];
        } else if ([kind isEqual:@"notify"]) {
            // Chuông: vòm + thân loe + vành + quả lắc
            CGContextFillEllipseInRect(ctx, CGRectMake(9, 6, 11, 12));
            CGContextMoveToPoint(ctx, 9, 12);
            CGContextAddLineToPoint(ctx, 20, 12);
            CGContextAddLineToPoint(ctx, 21.5, 19);
            CGContextAddLineToPoint(ctx, 7.5, 19);
            CGContextClosePath(ctx);
            CGContextFillPath(ctx);
            CGContextFillRect(ctx, CGRectMake(6.5, 18.5, 16, 2));
            CGContextFillEllipseInRect(ctx, CGRectMake(12.5, 20.5, 4, 3.5));
        } else if ([kind isEqual:@"ahead"]) {
            // Mũi tên tải xuống + vạch đáy
            CGContextFillRect(ctx, CGRectMake(13.2, 6, 2.6, 9));
            CGContextMoveToPoint(ctx, 8.5, 13.5);
            CGContextAddLineToPoint(ctx, 20.5, 13.5);
            CGContextAddLineToPoint(ctx, 14.5, 19.5);
            CGContextClosePath(ctx);
            CGContextFillPath(ctx);
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(7.5, 21, 14, 2.2) cornerRadius:1.1] fill];
        } else if ([kind isEqual:@"lyrics"]) {
            // Các dòng chữ (lời bài hát), dòng giữa ngắn
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(7, 8, 15, 2.4) cornerRadius:1.2] fill];
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(7, 13.3, 10, 2.4) cornerRadius:1.2] fill];
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(7, 18.6, 15, 2.4) cornerRadius:1.2] fill];
            CGContextFillEllipseInRect(ctx, CGRectMake(18.5, 12.5, 4, 4));
        } else if ([kind isEqual:@"music"]) {
            // Hai nốt móc đơn nối gạch
            CGContextFillEllipseInRect(ctx, CGRectMake(7.5, 17.5, 6.5, 5));
            CGContextFillEllipseInRect(ctx, CGRectMake(15.5, 15.5, 6.5, 5));
            CGContextFillRect(ctx, CGRectMake(12.4, 8, 1.6, 12));
            CGContextFillRect(ctx, CGRectMake(20.4, 6, 1.6, 12));
            CGContextMoveToPoint(ctx, 12.4, 8);
            CGContextAddLineToPoint(ctx, 22, 6);
            CGContextAddLineToPoint(ctx, 22, 9);
            CGContextAddLineToPoint(ctx, 12.4, 11);
            CGContextClosePath(ctx);
            CGContextFillPath(ctx);
        } else if ([kind isEqual:@"storage"]) {
            // Biểu đồ tròn: vòng + một múi đặc
            CGContextSetLineWidth(ctx, 1.8);
            CGContextStrokeEllipseInRect(ctx, CGRectMake(6, 6, 17, 17));
            CGContextMoveToPoint(ctx, 14.5, 14.5);
            CGContextAddArc(ctx, 14.5, 14.5, 8.5, -M_PI_2, 0.35, 0);
            CGContextClosePath(ctx);
            CGContextFillPath(ctx);
        } else {
            CGContextSetLineWidth(ctx, 1.6);
            CGContextStrokeEllipseInRect(ctx, CGRectMake(5.5, 5.5, 18, 18));
            CGContextFillEllipseInRect(ctx, CGRectMake(13.4, 9, 2.2, 2.2));
            CGContextFillRect(ctx, CGRectMake(13.5, 12.5, 2, 7));
        }
        // Vệt bóng nửa trên
        GMFillGradient(ctx, sq, CGRectMake(0, 0, 29, 14), @[RGBA(0xFFFFFF, 0.45), RGBA(0xFFFFFF, 0.1)], NULL);
    });
    [cache setObject:img forKey:kind];
    return img;
}

+ (UIImage *)folderIcon {
    static UIImage *img;
    if (!img) {
        img = GMDraw(CGSizeMake(36, 36), ^(CGContextRef ctx) {
            [RGB(0x5AA9F0) setFill];
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(2, 7, 14, 8) cornerRadius:2] fill];
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(2, 10, 32, 21) cornerRadius:3] fill];
            [RGB(0x8CC4F5) setFill];
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(2, 14, 32, 17) cornerRadius:3] fill];
        });
    }
    return img;
}

+ (UIImage *)fileIcon:(NSString *)ext {
    NSString *e = ext.length ? ext.uppercaseString : @"TỆP";
    if (e.length > 4) e = [e substringToIndex:4];
    static NSMutableDictionary *cache;
    if (!cache) cache = [NSMutableDictionary dictionary];
    NSString *ckey = gDark ? [e stringByAppendingString:@"|d"] : e;
    UIImage *img = [cache objectForKey:ckey];
    if (img) return img;
    NSString *l = e.lowercaseString;
    UIColor *bg = [@[@"jpg", @"jpeg", @"png", @"gif", @"heic", @"bmp", @"tif", @"tiff"] containsObject:l] ? RGB(0x37B35D)
                  : [@[@"mp4", @"mov", @"m4v", @"3gp", @"avi", @"mkv", @"webm"] containsObject:l]  ? RGB(0x9B59D0)
                  : [@[@"mp3", @"m4a", @"aac", @"wav", @"ogg", @"opus", @"flac", @"amr"] containsObject:l] ? RGB(0xF0A030)
                  : [@[@"pdf"] containsObject:l]                                                   ? RGB(0xE0483E)
                  : [@[@"zip", @"rar", @"7z", @"gz", @"tar", @"deb", @"ipa"] containsObject:l]     ? RGB(0x8E8E93)
                                                                                                   : [self accent];
    img = GMDraw(CGSizeMake(36, 36), ^(CGContextRef ctx) {
        [bg setFill];
        [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(2, 2, 32, 32) cornerRadius:7] fill];
        UIFont *f = [UIFont boldSystemFontOfSize:e.length > 3 ? 9.5 : 11];
        CGSize s = [e sizeWithFont:f];
        [[UIColor whiteColor] set];
        [e drawAtPoint:CGPointMake(18 - s.width / 2, 18 - s.height / 2) withFont:f];
    });
    [cache setObject:img forKey:ckey];
    return img;
}

static NSString *GMGrouped(long long n) {
    // 12.345 (dấu chấm ngăn nghìn kiểu Việt)
    NSString *s = [NSString stringWithFormat:@"%lld", n];
    NSMutableString *out = [NSMutableString string];
    for (NSUInteger i = 0; i < s.length; i++) {
        if (i > 0 && (s.length - i) % 3 == 0) [out appendString:@"."];
        [out appendFormat:@"%C", [s characterAtIndex:i]];
    }
    return out;
}

+ (NSString *)statusText:(NSDictionary *)st {
    NSString *kind = GMVal(st, @"kind");
    if ([kind isEqual:@"group"] || [kind isEqual:@"channel"]) {
        long long members = [GMVal(st, @"members") longLongValue], online = [GMVal(st, @"online") longLongValue];
        if (!GMVal(st, @"members")) return [kind isEqual:@"channel"] ? @"kênh" : @"nhóm";
        NSString *m = [NSString stringWithFormat:[kind isEqual:@"channel"] ? @"%@ người theo dõi" : @"%@ thành viên", GMGrouped(members)];
        return online > 1 ? [NSString stringWithFormat:@"%@, %@ trực tuyến", m, GMGrouped(online)] : m;
    }
    if ([GMVal(st, @"bot") boolValue]) return @"bot";
    NSString *s = GMVal(st, @"status");
    if ([s isEqual:@"online"]) return @"trực tuyến";
    if ([s isEqual:@"recently"]) return @"truy cập gần đây";
    if ([s isEqual:@"week"]) return @"truy cập trong tuần";
    if ([s isEqual:@"month"]) return @"truy cập trong tháng";
    if (![s isEqual:@"offline"]) return @"truy cập lâu rồi";
    NSDate *d = [NSDate dateWithTimeIntervalSince1970:[GMVal(st, @"was_online") doubleValue]];
    NSTimeInterval ago = -[d timeIntervalSinceNow];
    if (ago < 60) return @"vừa truy cập";
    if (ago < 3600) return [NSString stringWithFormat:@"truy cập %d phút trước", (int)(ago / 60)];
    NSCalendar *cal = [NSCalendar currentCalendar];
    NSDateComponents *a = [cal components:NSYearCalendarUnit | NSMonthCalendarUnit | NSDayCalendarUnit fromDate:d];
    NSDateComponents *b = [cal components:NSYearCalendarUnit | NSMonthCalendarUnit | NSDayCalendarUnit fromDate:[NSDate date]];
    NSDateFormatter *f = [[NSDateFormatter alloc] init];
    f.dateFormat = @"HH:mm";
    NSString *hm = [f stringFromDate:d];
    BOOL sameMonth = a.year == b.year && a.month == b.month;
    if (sameMonth && a.day == b.day) return [@"truy cập lúc " stringByAppendingString:hm];
    if (ago < 2 * 86400 && sameMonth && b.day - a.day == 1) return [@"truy cập hôm qua lúc " stringByAppendingString:hm];
    f.dateFormat = @"dd/MM/yy";
    return [@"truy cập " stringByAppendingString:[f stringFromDate:d]];
}

+ (NSString *)typingText:(NSString *)action name:(NSString *)name {
    NSDictionary *t = @{@"recording": @"đang ghi âm…", @"voice": @"đang gửi tin thoại…", @"photo": @"đang gửi ảnh…",
                        @"video": @"đang gửi video…", @"round": @"đang quay video…", @"file": @"đang gửi tệp…",
                        @"sticker": @"đang chọn sticker…", @"game": @"đang chơi game…"};
    NSString *what = [t objectForKey:action] ?: @"đang gõ…";
    if (!name.length) return what;
    NSString *first = [[name componentsSeparatedByString:@" "] objectAtIndex:0];
    return [NSString stringWithFormat:@"%@ %@", first, what];
}

+ (NSString *)sizeText:(double)b {
    NSString *s;
    if (b < 1024) s = [NSString stringWithFormat:@"%.0f B", b];
    else if (b < 1024 * 1024) s = [NSString stringWithFormat:@"%.0f KB", b / 1024];
    else if (b < 1024.0 * 1024 * 1024) s = [NSString stringWithFormat:@"%.1f MB", b / 1048576];
    else s = [NSString stringWithFormat:@"%.2f GB", b / 1073741824.0];
    return [s stringByReplacingOccurrencesOfString:@"." withString:@","];
}

+ (NSString *)ttlText:(NSInteger)s {
    if (s <= 0) return @"Tắt";
    if (s == 86400) return @"1 ngày";
    if (s == 7 * 86400) return @"1 tuần";
    if (s == 31 * 86400) return @"1 tháng";
    if (s % (7 * 86400) == 0) return [NSString stringWithFormat:@"%ld tuần", (long)(s / (7 * 86400))];
    if (s % 86400 == 0) return [NSString stringWithFormat:@"%ld ngày", (long)(s / 86400)];
    if (s % 3600 == 0) return [NSString stringWithFormat:@"%ld giờ", (long)(s / 3600)];
    if (s % 60 == 0) return [NSString stringWithFormat:@"%ld phút", (long)(s / 60)];
    return [NSString stringWithFormat:@"%ld giây", (long)s];
}

+ (NSArray *)ttlChoices {
    return @[@0, @86400, @(7 * 86400), @(31 * 86400)];
}

+ (UIButton *)button:(NSString *)title {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeCustom];
    [b setTitle:title forState:UIControlStateNormal];
    [b setTitleColor:[UIColor whiteColor] forState:UIControlStateNormal];
    [b setTitleColor:[UIColor colorWithWhite:1 alpha:0.5] forState:UIControlStateHighlighted];
    [b setTitleColor:[UIColor colorWithWhite:1 alpha:0.6] forState:UIControlStateDisabled];
    b.titleLabel.font = [UIFont boldSystemFontOfSize:17];
    // Nút xanh bóng kiểu iOS 6 (cùng ảnh với nút «Gửi»: cao 44, viên giữa cao 32)
    [b setBackgroundImage:[self sendButtonImage:NO] forState:UIControlStateNormal];
    [b setBackgroundImage:[self sendButtonImage:YES] forState:UIControlStateHighlighted];
    [b setTitleShadowColor:[UIColor colorWithWhite:0 alpha:0.35] forState:UIControlStateNormal];
    b.titleLabel.shadowOffset = CGSizeMake(0, -1);
    b.adjustsImageWhenHighlighted = NO;
    return b;
}

@end
