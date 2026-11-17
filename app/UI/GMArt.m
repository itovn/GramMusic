#import "GMArt.h"
#import "GMTheme.h"

NSString *GMDuration(double s) {
    if (s != s || s < 0) s = 0;  // NaN
    long t = (long)(s + 0.5);
    if (t >= 3600) return [NSString stringWithFormat:@"%ld:%02ld:%02ld", t / 3600, (t / 60) % 60, t % 60];
    return [NSString stringWithFormat:@"%ld:%02ld", t / 60, t % 60];
}

static UIImage *GMArtDraw(CGSize size, void (^draw)(CGContextRef ctx)) {
    UIGraphicsBeginImageContextWithOptions(size, NO, 0);
    draw(UIGraphicsGetCurrentContext());
    UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
    return img;
}

static UIColor *GMHex(int hex, CGFloat a) {
    return [UIColor colorWithRed:((hex >> 16) & 255) / 255.0 green:((hex >> 8) & 255) / 255.0 blue:(hex & 255) / 255.0 alpha:a];
}

@implementation GMArt

+ (UIImage *)glyph:(NSString *)name size:(CGFloat)size color:(UIColor *)color {
    static NSCache *cache;
    if (!cache) cache = [[NSCache alloc] init];
    NSString *ck = [NSString stringWithFormat:@"%@|%.0f|%@", name, size, color];
    UIImage *hit = [cache objectForKey:ck];
    if (hit) return hit;
    UIImage *img = GMArtDraw(CGSizeMake(size, size), ^(CGContextRef ctx) {
        // Vẽ trong hệ 24 × 24 rồi phóng theo cỡ
        CGFloat k = size / 24.0;
        CGContextScaleCTM(ctx, k, k);
        [color set];
        CGContextSetLineCap(ctx, kCGLineCapRound);
        CGContextSetLineJoin(ctx, kCGLineJoinRound);
        CGContextSetLineWidth(ctx, 2);
        if ([name isEqual:@"play"]) {
            UIBezierPath *p = [UIBezierPath bezierPath];
            [p moveToPoint:CGPointMake(7, 4)];
            [p addLineToPoint:CGPointMake(20, 12)];
            [p addLineToPoint:CGPointMake(7, 20)];
            [p closePath];
            p.lineJoinStyle = kCGLineJoinRound;
            p.lineWidth = 1.5;
            [p fill];
            [p stroke];
        } else if ([name isEqual:@"pause"]) {
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(6, 4.5, 4, 15) cornerRadius:1] fill];
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(14, 4.5, 4, 15) cornerRadius:1] fill];
        } else if ([name isEqual:@"next"] || [name isEqual:@"prev"]) {
            if ([name isEqual:@"prev"]) {
                CGContextTranslateCTM(ctx, 24, 0);
                CGContextScaleCTM(ctx, -1, 1);
            }
            for (int i = 0; i < 2; i++) {
                CGFloat x = 3 + i * 8;
                UIBezierPath *p = [UIBezierPath bezierPath];
                [p moveToPoint:CGPointMake(x, 6)];
                [p addLineToPoint:CGPointMake(x + 8.5, 12)];
                [p addLineToPoint:CGPointMake(x, 18)];
                [p closePath];
                [p fill];
            }
            [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(19.5, 6, 2.2, 12) cornerRadius:0.8] fill];
        } else if ([name isEqual:@"shuffle"]) {
            CGContextSetLineWidth(ctx, 1.8);
            CGContextMoveToPoint(ctx, 3, 7);
            CGContextAddCurveToPoint(ctx, 11, 7, 12, 17, 19, 17);
            CGContextMoveToPoint(ctx, 3, 17);
            CGContextAddCurveToPoint(ctx, 11, 17, 12, 7, 19, 7);
            CGContextStrokePath(ctx);
            for (int i = 0; i < 2; i++) {
                CGFloat y = i ? 17 : 7;
                CGContextMoveToPoint(ctx, 17, y - 3);
                CGContextAddLineToPoint(ctx, 21, y);
                CGContextAddLineToPoint(ctx, 17, y + 3);
            }
            CGContextStrokePath(ctx);
        } else if ([name isEqual:@"repeat"] || [name isEqual:@"repeat1"]) {
            CGContextSetLineWidth(ctx, 1.8);
            UIBezierPath *p = [UIBezierPath bezierPathWithRoundedRect:CGRectMake(4, 7, 16, 10) cornerRadius:4];
            p.lineWidth = 1.8;
            [p stroke];
            // mũi tên trên (sang phải), dưới (sang trái) — xoá một khúc dưới mũi tên
            CGContextMoveToPoint(ctx, 13, 4);
            CGContextAddLineToPoint(ctx, 16, 7);
            CGContextAddLineToPoint(ctx, 13, 10);
            CGContextMoveToPoint(ctx, 11, 14);
            CGContextAddLineToPoint(ctx, 8, 17);
            CGContextAddLineToPoint(ctx, 11, 20);
            CGContextStrokePath(ctx);
            if ([name isEqual:@"repeat1"]) {
                UIFont *f = [UIFont boldSystemFontOfSize:7];
                [@"1" drawInRect:CGRectMake(9.5, 8, 5, 8) withFont:f lineBreakMode:0 alignment:GMAlignCenter];
            }
        } else if ([name isEqual:@"heart"] || [name isEqual:@"heartFill"]) {
            UIBezierPath *p = [UIBezierPath bezierPath];
            [p moveToPoint:CGPointMake(12, 20)];
            [p addCurveToPoint:CGPointMake(3, 9) controlPoint1:CGPointMake(7, 16) controlPoint2:CGPointMake(3, 13)];
            [p addCurveToPoint:CGPointMake(12, 7) controlPoint1:CGPointMake(3, 4) controlPoint2:CGPointMake(9.5, 3)];
            [p addCurveToPoint:CGPointMake(21, 9) controlPoint1:CGPointMake(14.5, 3) controlPoint2:CGPointMake(21, 4)];
            [p addCurveToPoint:CGPointMake(12, 20) controlPoint1:CGPointMake(21, 13) controlPoint2:CGPointMake(17, 16)];
            [p closePath];
            p.lineWidth = 1.8;
            p.lineJoinStyle = kCGLineJoinRound;
            if ([name isEqual:@"heartFill"]) [p fill];
            [p stroke];
        } else if ([name isEqual:@"queue"]) {
            CGContextSetLineWidth(ctx, 1.8);
            // Vẽ nét trước rồi mới tô chấm (CGContextFillEllipseInRect xoá đường đang dựng)
            for (int i = 0; i < 3; i++) {
                CGFloat y = 6 + i * 6;
                CGContextMoveToPoint(ctx, 8, y);
                CGContextAddLineToPoint(ctx, 21, y);
            }
            CGContextStrokePath(ctx);
            for (int i = 0; i < 3; i++) CGContextFillEllipseInRect(ctx, CGRectMake(2.5, 6 + i * 6 - 1.5, 3, 3));
        } else if ([name isEqual:@"download"] || [name isEqual:@"downloaded"]) {
            CGContextSetLineWidth(ctx, 1.8);
            if ([name isEqual:@"downloaded"]) {
                CGContextFillEllipseInRect(ctx, CGRectMake(2, 2, 20, 20));
                [[UIColor whiteColor] set];
                CGContextSetLineWidth(ctx, 2.2);
            }
            CGContextMoveToPoint(ctx, 12, 5.5);
            CGContextAddLineToPoint(ctx, 12, 15.5);
            CGContextMoveToPoint(ctx, 7.5, 11.5);
            CGContextAddLineToPoint(ctx, 12, 16);
            CGContextAddLineToPoint(ctx, 16.5, 11.5);
            if (![name isEqual:@"downloaded"]) {
                CGContextMoveToPoint(ctx, 4.5, 16);
                CGContextAddLineToPoint(ctx, 4.5, 19.5);
                CGContextAddLineToPoint(ctx, 19.5, 19.5);
                CGContextAddLineToPoint(ctx, 19.5, 16);
            }
            CGContextStrokePath(ctx);
        } else if ([name isEqual:@"more"]) {
            for (int i = 0; i < 3; i++) CGContextFillEllipseInRect(ctx, CGRectMake(4 + i * 6.5, 10.5, 3.2, 3.2));
        } else if ([name isEqual:@"lyrics"]) {
            // Lời bài hát: ba dòng chữ + nốt nhạc nhỏ góc dưới
            CGContextSetLineWidth(ctx, 2);
            CGContextSetLineCap(ctx, kCGLineCapRound);
            CGContextMoveToPoint(ctx, 4, 6);
            CGContextAddLineToPoint(ctx, 20, 6);
            CGContextMoveToPoint(ctx, 4, 11);
            CGContextAddLineToPoint(ctx, 15, 11);
            CGContextMoveToPoint(ctx, 4, 16);
            CGContextAddLineToPoint(ctx, 11, 16);
            CGContextStrokePath(ctx);
            CGContextFillEllipseInRect(ctx, CGRectMake(13.5, 16.5, 5, 4));
            CGContextSetLineWidth(ctx, 1.6);
            CGContextMoveToPoint(ctx, 18, 18.5);
            CGContextAddLineToPoint(ctx, 18, 10.5);
            CGContextAddLineToPoint(ctx, 20.5, 11.5);
            CGContextStrokePath(ctx);
        } else if ([name isEqual:@"timer"]) {
            CGContextSetLineWidth(ctx, 1.8);
            CGContextStrokeEllipseInRect(ctx, CGRectMake(4, 5, 16, 16));
            CGContextMoveToPoint(ctx, 12, 8.5);
            CGContextAddLineToPoint(ctx, 12, 13);
            CGContextAddLineToPoint(ctx, 15, 15);
            CGContextMoveToPoint(ctx, 9.5, 2.5);
            CGContextAddLineToPoint(ctx, 14.5, 2.5);
            CGContextStrokePath(ctx);
        } else if ([name isEqual:@"plus"]) {
            CGContextMoveToPoint(ctx, 12, 4);
            CGContextAddLineToPoint(ctx, 12, 20);
            CGContextMoveToPoint(ctx, 4, 12);
            CGContextAddLineToPoint(ctx, 20, 12);
            CGContextStrokePath(ctx);
        } else if ([name isEqual:@"close"]) {
            // mũi tên xuống (đóng màn Đang phát)
            CGContextSetLineWidth(ctx, 2.4);
            CGContextMoveToPoint(ctx, 5, 9);
            CGContextAddLineToPoint(ctx, 12, 16);
            CGContextAddLineToPoint(ctx, 19, 9);
            CGContextStrokePath(ctx);
        } else if ([name isEqual:@"search"]) {
            CGContextSetLineWidth(ctx, 2);
            CGContextStrokeEllipseInRect(ctx, CGRectMake(4, 4, 11, 11));
            CGContextMoveToPoint(ctx, 13.5, 13.5);
            CGContextAddLineToPoint(ctx, 20, 20);
            CGContextStrokePath(ctx);
        } else {
            // note
            CGContextSetLineWidth(ctx, 1.8);
            CGContextMoveToPoint(ctx, 9, 18);
            CGContextAddLineToPoint(ctx, 9, 5);
            CGContextAddLineToPoint(ctx, 19, 3);
            CGContextAddLineToPoint(ctx, 19, 15);
            CGContextStrokePath(ctx);
            CGContextFillEllipseInRect(ctx, CGRectMake(3.5, 15, 7, 5.5));
            CGContextFillEllipseInRect(ctx, CGRectMake(13.5, 12, 7, 5.5));
        }
    });
    [cache setObject:img forKey:ck];
    return img;
}

+ (UIImage *)placeholderFor:(NSString *)key size:(CGFloat)size {
    static NSCache *cache;
    if (!cache) cache = [[NSCache alloc] init];
    static int pairs[][2] = {
        {0x8E7CF0, 0x5743C9}, {0xF39A6B, 0xD9534F}, {0x5CC6E8, 0x2A7FC2}, {0x7FD17A, 0x2E9A55},
        {0xF5C35B, 0xE0822B}, {0xEE7AAE, 0xB0447F}, {0x6E8FAF, 0x34495E},
    };
    NSUInteger h = 0;
    for (NSUInteger i = 0; i < key.length; i++) h = h * 31 + [key characterAtIndex:i];
    int *pair = pairs[h % 7];
    NSString *ck = [NSString stringWithFormat:@"%lu|%.0f", (unsigned long)(h % 7), size];
    UIImage *hit = [cache objectForKey:ck];
    if (hit) return hit;
    UIImage *img = GMArtDraw(CGSizeMake(size, size), ^(CGContextRef ctx) {
        CGRect r = CGRectMake(0, 0, size, size);
        UIBezierPath *p = [UIBezierPath bezierPathWithRoundedRect:r cornerRadius:size * 0.08];
        [p addClip];
        CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
        NSArray *cols = @[(__bridge id)GMHex(pair[0], 1).CGColor, (__bridge id)GMHex(pair[1], 1).CGColor];
        CGGradientRef g = CGGradientCreateWithColors(cs, (__bridge CFArrayRef)cols, NULL);
        CGContextDrawLinearGradient(ctx, g, CGPointZero, CGPointMake(size, size), 0);
        CGGradientRelease(g);
        CGColorSpaceRelease(cs);
        // Vệt bóng nửa trên (kính iOS 6)
        [GMHex(0xFFFFFF, 0.14) setFill];
        CGContextFillRect(ctx, CGRectMake(0, 0, size, size * 0.45));
        UIImage *note = [self glyph:@"note" size:size * 0.5 color:GMHex(0xFFFFFF, 0.9)];
        [note drawAtPoint:CGPointMake(size * 0.25, size * 0.25)];
    });
    [cache setObject:img forKey:ck];
    return img;
}

+ (UIImage *)sliderThumb {
    static UIImage *img;
    if (img) return img;
    img = GMArtDraw(CGSizeMake(22, 22), ^(CGContextRef ctx) {
        CGContextSetShadowWithColor(ctx, CGSizeMake(0, 1), 2, GMHex(0x000000, 0.4).CGColor);
        [[UIColor whiteColor] setFill];
        CGContextFillEllipseInRect(ctx, CGRectMake(3, 2, 16, 16));
        CGContextSetShadowWithColor(ctx, CGSizeZero, 0, NULL);
        [GMHex(0x000000, 0.18) setStroke];
        CGContextSetLineWidth(ctx, 1);
        CGContextStrokeEllipseInRect(ctx, CGRectMake(3.5, 2.5, 15, 15));
    });
    return img;
}

+ (UIImage *)sliderTrack:(BOOL)played {
    UIColor *c = played ? [GMTheme accent] : ([GMTheme dark] ? GMHex(0xFFFFFF, 0.22) : GMHex(0x3C506A, 0.25));
    UIImage *img = GMArtDraw(CGSizeMake(9, 4), ^(CGContextRef ctx) {
        [c setFill];
        [[UIBezierPath bezierPathWithRoundedRect:CGRectMake(0, 0, 9, 4) cornerRadius:2] fill];
    });
    return [img resizableImageWithCapInsets:UIEdgeInsetsMake(0, 4, 0, 4)];
}

+ (UIImage *)badge:(NSString *)text color:(UIColor *)color {
    static NSCache *cache;
    if (!cache) cache = [[NSCache alloc] init];
    NSString *ck = [NSString stringWithFormat:@"%@|%@", text, color];
    UIImage *hit = [cache objectForKey:ck];
    if (hit) return hit;
    UIFont *f = [UIFont boldSystemFontOfSize:9];
    CGSize s = [text sizeWithFont:f];
    CGSize size = CGSizeMake(ceil(s.width) + 8, 13);
    UIImage *img = GMArtDraw(size, ^(CGContextRef ctx) {
        UIBezierPath *p = [UIBezierPath bezierPathWithRoundedRect:CGRectMake(0.5, 0.5, size.width - 1, size.height - 1) cornerRadius:3];
        [color setStroke];
        p.lineWidth = 1;
        [p stroke];
        [color set];
        [text drawInRect:CGRectMake(0, 0.5, size.width, size.height) withFont:f lineBreakMode:0 alignment:GMAlignCenter];
    });
    [cache setObject:img forKey:ck];
    return img;
}

@end
