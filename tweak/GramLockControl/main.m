// GramLockControl — chạy trong SpringBoard (iOS 5 / 6). Xem Makefile và docs/DECISIONS.md M20.
//
// Màn hình khoá iOS 5 / 6 chỉ có thanh tiến trình cho app Nhạc, nút ⏮ ⏯ ⏭ phải bấm Home 2 lần mới hiện. Tweak thêm một khung nhỏ phía
// trên «trượt để mở khoá» khi GramMusic là app đang phát: lời đang hát (dòng hiện tại + dòng kế), thanh tiến trình kéo để tua, nút bài
// trước / phát / bài sau.
//
// - Dữ liệu: app ghi trạng thái (bài, vị trí lúc ghi, đang phát?, lời chạy) vào ~/Library/GramMusic/lockstate.plist mỗi khi có thay đổi
//   rồi báo «vn.grammusic.lockcontrol/state» (Darwin); tweak tự tính vị trí hiện tại = vị trí lúc ghi + thời gian đã trôi.
// - Lệnh: phát / dừng, bài trước / sau qua SBMediaController (đường của iOS — đánh thức được app đang bị treo ở nền); không có thì báo
//   app «…/cmd.toggle|next|prev». Tua: «…/cmd.seek» kèm số mili giây (notify_set_state).
// - Bật / tắt (Cài đặt → GramLockControl, PreferenceLoader): tắt là gỡ khung, không làm gì nữa (phòng xung đột với tweak khác).
// - Móc bằng runtime, tra lúc chạy: thiếu lớp / hàm (iOS khác) thì đứng ngoài, không làm SpringBoard lỗi.
#import <QuartzCore/QuartzCore.h>
#import <UIKit/UIKit.h>
#import <objc/runtime.h>
#include <notify.h>

static NSString *const kApp = @"vn.grammusic.app";
static NSString *const kPrefsPath = @"/var/mobile/Library/Preferences/vn.grammusic.lockcontrol.plist";
static NSString *const kStatePath = @"/var/mobile/Library/GramMusic/lockstate.plist";
#define kNotePrefs "vn.grammusic.lockcontrol/prefs"
#define kNoteState "vn.grammusic.lockcontrol/state"
#define kNoteCmd "vn.grammusic.lockcontrol/cmd."
static const NSInteger kPanelTag = 0x474C43;

// Hàm riêng của SpringBoard (chỉ khai báo để gọi — luôn hỏi respondsToSelector trước)
@interface NSObject (GLCPrivate)
+ (id)sharedInstance;
+ (id)sharedAwayController;
- (id)nowPlayingApplication;
- (NSString *)bundleIdentifier;
- (NSString *)displayIdentifier;
- (BOOL)togglePlayPause;
- (BOOL)changeTrack:(int)delta;
- (void)restartDimTimer;
- (BOOL)isDimmed;
@end

static BOOL gEnabled = YES, gLyrics = YES, gProgress = YES, gControls = YES;
static NSDictionary *gState;
static Class gAwayClass;
static IMP gOrigMove, gOrigLayout;

#pragma mark - Nhật ký

static void glc_log(NSString *s) {
    NSString *dir = @"/var/mobile/Library/GramMusic";
    BOOL isDir = NO;
    if (![[NSFileManager defaultManager] fileExistsAtPath:dir isDirectory:&isDir] || !isDir) return;
    NSString *path = [dir stringByAppendingPathComponent:@"glc.log"];
    NSDictionary *a = [[NSFileManager defaultManager] attributesOfItemAtPath:path error:NULL];
    if ([a fileSize] > 256 * 1024) [[NSFileManager defaultManager] removeItemAtPath:path error:NULL];
    NSString *line = [NSString stringWithFormat:@"%.3f %@\n", [[NSDate date] timeIntervalSince1970], s];
    NSFileHandle *h = [NSFileHandle fileHandleForWritingAtPath:path];
    if (!h) {
        [line writeToFile:path atomically:NO encoding:NSUTF8StringEncoding error:NULL];
        return;
    }
    [h seekToEndOfFile];
    [h writeData:[line dataUsingEncoding:NSUTF8StringEncoding]];
    [h closeFile];
}

#pragma mark - Cài đặt, trạng thái

static BOOL glc_bool(NSDictionary *d, NSString *k) {
    id v = [d objectForKey:k];
    return v ? [v boolValue] : YES;  // mặc định bật
}

static void glc_loadPrefs(void) {
    NSDictionary *d = [NSDictionary dictionaryWithContentsOfFile:kPrefsPath];
    gEnabled = glc_bool(d, @"enabled");
    gLyrics = glc_bool(d, @"lyrics");
    gProgress = glc_bool(d, @"progress");
    gControls = glc_bool(d, @"controls");
}

static void glc_loadState(void) {
    gState = [NSDictionary dictionaryWithContentsOfFile:kStatePath];
}

static double glc_num(NSString *k) {
    return [[gState objectForKey:k] doubleValue];
}

static BOOL glc_playing(void) {
    return [[gState objectForKey:@"playing"] boolValue];
}

/// Vị trí hiện tại = vị trí lúc app ghi + thời gian đã trôi (nếu đang phát).
static double glc_pos(void) {
    double pos = glc_num(@"pos");
    if (glc_playing()) pos += MAX(0, [[NSDate date] timeIntervalSince1970] - glc_num(@"at"));
    double dur = glc_num(@"dur");
    return dur > 0 ? MIN(pos, dur) : pos;
}

static id glc_media(void) {
    Class c = objc_getClass("SBMediaController");
    return [c respondsToSelector:@selector(sharedInstance)] ? [c sharedInstance] : nil;
}

static id glc_away(void) {
    Class c = objc_getClass("SBAwayController");
    return [c respondsToSelector:@selector(sharedAwayController)] ? [c sharedAwayController] : nil;
}

/// GramMusic có phải app đang phát (theo iOS)? Không hỏi được thì tin vào trạng thái app ghi (12 giờ).
static BOOL glc_gramIsNowPlaying(void) {
    id m = glc_media();
    if ([m respondsToSelector:@selector(nowPlayingApplication)]) {
        id app = [m nowPlayingApplication];
        NSString *bid = [app respondsToSelector:@selector(bundleIdentifier)] ? [app bundleIdentifier]
                        : [app respondsToSelector:@selector(displayIdentifier)] ? [app displayIdentifier] : nil;
        return [bid isEqualToString:kApp];
    }
    return [[NSDate date] timeIntervalSince1970] - glc_num(@"at") < 12 * 3600;
}

static BOOL glc_shouldShow(void) {
    return gEnabled && [[gState objectForKey:@"key"] length] > 0 && glc_gramIsNowPlaying();
}

#pragma mark - Lệnh tới app

static void glc_post(NSString *cmd) {
    notify_post([[@kNoteCmd stringByAppendingString:cmd] UTF8String]);
}

static void glc_poke(void) {
    // Chạm vào khung: màn hình khoá đừng tối đi giữa chừng
    id aw = glc_away();
    if ([aw respondsToSelector:@selector(restartDimTimer)]) [aw restartDimTimer];
}

static void glc_toggle(void) {
    id m = glc_media();
    if (glc_gramIsNowPlaying() && [m respondsToSelector:@selector(togglePlayPause)]) [m togglePlayPause];
    else glc_post(@"toggle");
    // Hiện ngay (app sẽ ghi trạng thái thật sau)
    NSMutableDictionary *s = [gState mutableCopy];
    [s setObject:@(glc_pos()) forKey:@"pos"];
    [s setObject:@([[NSDate date] timeIntervalSince1970]) forKey:@"at"];
    [s setObject:@(!glc_playing()) forKey:@"playing"];
    gState = s;
}

static void glc_track(int delta) {
    id m = glc_media();
    if (glc_gramIsNowPlaying() && [m respondsToSelector:@selector(changeTrack:)]) [m changeTrack:delta];
    else glc_post(delta > 0 ? @"next" : @"prev");
}

static void glc_seek(double sec) {
    int tok = 0;
    if (notify_register_check(kNoteCmd "seek", &tok) == NOTIFY_STATUS_OK) {
        notify_set_state(tok, (uint64_t)(MAX(0, sec) * 1000));
        notify_post(kNoteCmd "seek");
        notify_cancel(tok);
    }
    NSMutableDictionary *s = [gState mutableCopy];
    [s setObject:@(sec) forKey:@"pos"];
    [s setObject:@([[NSDate date] timeIntervalSince1970]) forKey:@"at"];
    gState = s;
}

#pragma mark - Hình nút

static UIImage *glc_glyph(NSString *kind, CGFloat size) {
    static NSMutableDictionary *cache;
    if (!cache) cache = [NSMutableDictionary dictionary];
    NSString *ck = [NSString stringWithFormat:@"%@|%.0f", kind, size];
    UIImage *hit = [cache objectForKey:ck];
    if (hit) return hit;
    UIGraphicsBeginImageContextWithOptions(CGSizeMake(size, size), NO, 0);
    CGContextRef ctx = UIGraphicsGetCurrentContext();
    [[UIColor whiteColor] set];
    CGFloat s = size / 24.0;
    CGContextScaleCTM(ctx, s, s);
    if ([kind isEqual:@"play"]) {
        CGContextMoveToPoint(ctx, 7, 4);
        CGContextAddLineToPoint(ctx, 20, 12);
        CGContextAddLineToPoint(ctx, 7, 20);
        CGContextFillPath(ctx);
    } else if ([kind isEqual:@"pause"]) {
        CGContextFillRect(ctx, CGRectMake(6, 4, 4, 16));
        CGContextFillRect(ctx, CGRectMake(14, 4, 4, 16));
    } else {
        BOOL next = [kind isEqual:@"next"];
        if (!next) {
            CGContextTranslateCTM(ctx, 24, 0);
            CGContextScaleCTM(ctx, -1, 1);
        }
        CGContextMoveToPoint(ctx, 3, 5);
        CGContextAddLineToPoint(ctx, 11, 12);
        CGContextAddLineToPoint(ctx, 3, 19);
        CGContextFillPath(ctx);
        CGContextMoveToPoint(ctx, 11, 5);
        CGContextAddLineToPoint(ctx, 19, 12);
        CGContextAddLineToPoint(ctx, 11, 19);
        CGContextFillPath(ctx);
        CGContextFillRect(ctx, CGRectMake(19, 5, 2.5, 14));
    }
    UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
    if (img) [cache setObject:img forKey:ck];
    return img;
}

static UIImage *glc_dot(CGFloat d) {
    UIGraphicsBeginImageContextWithOptions(CGSizeMake(d, d), NO, 0);
    [[UIColor whiteColor] set];
    CGContextFillEllipseInRect(UIGraphicsGetCurrentContext(), CGRectMake(0, 0, d, d));
    UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
    return img;
}

static UIImage *glc_track_img(UIColor *c) {
    UIGraphicsBeginImageContextWithOptions(CGSizeMake(9, 3), NO, 0);
    [c set];
    UIRectFill(CGRectMake(0, 0, 9, 3));
    UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
    return [img resizableImageWithCapInsets:UIEdgeInsetsMake(1, 4, 1, 4)];
}

static NSString *glc_time(double t) {
    int s = (int)MAX(0, t);
    return [NSString stringWithFormat:@"%d:%02d", s / 60, s % 60];
}

#pragma mark - Khung trên màn hình khoá

@interface GLCPanel : UIView
- (void)refresh;
- (CGFloat)heightForWidth:(CGFloat)w;
@end

@implementation GLCPanel {
    UILabel *_line1, *_line2, *_el, *_rem;
    UISlider *_slider;
    UIButton *_prev, *_play, *_next;
    NSTimer *_timer;
    NSUInteger _ticks;
    BOOL _dragging;
    BOOL _hasLines;
}

- (UILabel *)label:(UIFont *)f alpha:(CGFloat)a {
    UILabel *l = [[UILabel alloc] init];
    l.font = f;
    l.textColor = [UIColor colorWithWhite:1 alpha:a];
    l.backgroundColor = [UIColor clearColor];
    l.textAlignment = 1;  // giữa (số: NSTextAlignmentCenter của SDK ghi iOS 6)
    l.shadowColor = [UIColor colorWithWhite:0 alpha:0.6];
    l.shadowOffset = CGSizeMake(0, 1);
    [self addSubview:l];
    return l;
}

- (UIButton *)button:(NSString *)glyph action:(SEL)sel {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeCustom];
    [b setImage:glc_glyph(glyph, 24) forState:UIControlStateNormal];
    b.showsTouchWhenHighlighted = YES;
    [b addTarget:self action:sel forControlEvents:UIControlEventTouchUpInside];
    [self addSubview:b];
    return b;
}

- (id)initWithFrame:(CGRect)f {
    if ((self = [super initWithFrame:f])) {
        self.backgroundColor = [UIColor colorWithWhite:0 alpha:0.45];
        self.layer.cornerRadius = 10;
        self.clipsToBounds = YES;
        _line1 = [self label:[UIFont boldSystemFontOfSize:15] alpha:1];
        _line2 = [self label:[UIFont systemFontOfSize:12] alpha:0.6];
        _el = [self label:[UIFont systemFontOfSize:10] alpha:0.8];
        _rem = [self label:[UIFont systemFontOfSize:10] alpha:0.8];
        _slider = [[UISlider alloc] init];
        [_slider setThumbImage:glc_dot(12) forState:UIControlStateNormal];
        [_slider setMinimumTrackImage:glc_track_img([UIColor whiteColor]) forState:UIControlStateNormal];
        [_slider setMaximumTrackImage:glc_track_img([UIColor colorWithWhite:1 alpha:0.3]) forState:UIControlStateNormal];
        [_slider addTarget:self action:@selector(slideStart) forControlEvents:UIControlEventTouchDown];
        [_slider addTarget:self action:@selector(sliding) forControlEvents:UIControlEventValueChanged];
        [_slider addTarget:self action:@selector(slideEnd)
          forControlEvents:UIControlEventTouchUpInside | UIControlEventTouchUpOutside | UIControlEventTouchCancel];
        [self addSubview:_slider];
        _prev = [self button:@"prev" action:@selector(prev)];
        _play = [self button:@"play" action:@selector(toggle)];
        _next = [self button:@"next" action:@selector(next)];
    }
    return self;
}

- (void)dealloc {
    [_timer invalidate];
}

- (CGFloat)heightForWidth:(CGFloat)w {
    CGFloat h = 6;
    if (gLyrics && _hasLines) h += 40;
    if (gProgress) h += 32;
    if (gControls) h += 40;
    return h + 4;
}

- (void)layoutSubviews {
    [super layoutSubviews];
    CGFloat w = self.bounds.size.width, y = 6;
    BOOL lyr = gLyrics && _hasLines;
    _line1.hidden = _line2.hidden = !lyr;
    if (lyr) {
        _line1.frame = CGRectMake(8, y, w - 16, 20);
        _line2.frame = CGRectMake(8, y + 20, w - 16, 16);
        y += 40;
    }
    _slider.hidden = _el.hidden = _rem.hidden = !gProgress;
    if (gProgress) {
        _el.frame = CGRectMake(4, y + 4, 38, 14);
        _rem.frame = CGRectMake(w - 42, y + 4, 38, 14);
        _slider.frame = CGRectMake(44, y, w - 88, 22);
        y += 32;
    }
    _prev.hidden = _play.hidden = _next.hidden = !gControls;
    if (gControls) {
        CGFloat mid = w / 2;
        _play.frame = CGRectMake(mid - 25, y, 50, 36);
        _prev.frame = CGRectMake(mid - 95, y, 50, 36);
        _next.frame = CGRectMake(mid + 45, y, 50, 36);
    }
}

- (void)didMoveToWindow {
    [super didMoveToWindow];
    if (!self.window) [self stop];
}

- (void)stop {
    [_timer invalidate];
    _timer = nil;
}

- (void)refresh {
    BOOL show = glc_shouldShow();
    self.hidden = !show;
    if (!show || !self.window) {
        [self stop];
        return;
    }
    if (!_timer) _timer = [NSTimer scheduledTimerWithTimeInterval:0.5 target:self selector:@selector(tick) userInfo:nil repeats:YES];
    [self tick];
}

/// Dòng lời đang hát (sớm 0,3 giây cho kịp đọc): chỉ số dòng cuối cùng đã tới mốc, -1 = chưa tới dòng nào.
static NSInteger glc_lineAt(NSArray *lines, double t) {
    NSInteger lo = 0, hi = (NSInteger)lines.count - 1, idx = -1;
    while (lo <= hi) {
        NSInteger m = (lo + hi) / 2;
        NSArray *l = [lines objectAtIndex:m];
        if ([l isKindOfClass:[NSArray class]] && l.count >= 1 && [[l objectAtIndex:0] doubleValue] <= t + 0.3) {
            idx = m;
            lo = m + 1;
        } else {
            hi = m - 1;
        }
    }
    return idx;
}

static NSString *glc_text(NSArray *lines, NSInteger i) {
    if (i < 0 || i >= (NSInteger)lines.count) return @"";
    NSArray *l = [lines objectAtIndex:i];
    NSString *s = l.count >= 2 ? [l objectAtIndex:1] : @"";
    s = [s stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    return s.length ? s : @"♪";
}

- (void)tick {
    if (!self.window || self.hidden) {
        [self stop];
        return;
    }
    // GramMusic chết (iPad 1 hết RAM…) / app khác giành phát lúc màn hình khoá đang mở: tệp trạng thái vẫn ghi «đang phát» — 2 giây
    // hỏi lại iOS một lần, thôi thì ẩn khung
    if (++_ticks % 4 == 0 && !glc_shouldShow()) {
        [self refresh];
        return;
    }
    id aw = glc_away();
    if ([aw respondsToSelector:@selector(isDimmed)] && [aw isDimmed]) return;  // màn hình tắt: khỏi vẽ
    double pos = glc_pos(), dur = glc_num(@"dur");
    NSArray *lines = [gState objectForKey:@"lines"];
    BOOL synced = [lines isKindOfClass:[NSArray class]] && lines.count > 0;
    BOOL instrumental = !synced && [[gState objectForKey:@"instrumental"] boolValue];
    BOOL has = synced || instrumental;
    if (has != _hasLines) {
        _hasLines = has;
        // Đổi chiều cao khung (có / không có lời): xếp lại trên màn hình khoá
        [self.superview setNeedsLayout];
    }
    if (gLyrics && instrumental) {
        // LRCLIB ghi bài không lời: nói rõ, khỏi tưởng mất lời
        _line1.text = @"♪ Nhạc không lời";
        _line2.text = @"";
    } else if (gLyrics && has) {
        NSInteger i = glc_lineAt(lines, pos);
        _line1.text = i < 0 ? @"♪" : glc_text(lines, i);
        _line2.text = glc_text(lines, i + 1);
    }
    if (gProgress && !_dragging) {
        _slider.maximumValue = MAX(1, dur);
        _slider.value = pos;
        _el.text = glc_time(pos);
        _rem.text = dur > 0 ? [@"-" stringByAppendingString:glc_time(dur - pos)] : @"";
    }
    [_play setImage:glc_glyph(glc_playing() ? @"pause" : @"play", 24) forState:UIControlStateNormal];
}

- (void)slideStart {
    _dragging = YES;
    glc_poke();
}

- (void)sliding {
    _el.text = glc_time(_slider.value);
    double dur = glc_num(@"dur");
    _rem.text = dur > 0 ? [@"-" stringByAppendingString:glc_time(dur - _slider.value)] : @"";
    glc_poke();
}

- (void)slideEnd {
    _dragging = NO;
    glc_seek(_slider.value);
    glc_poke();
    [self tick];
}

- (void)toggle {
    glc_toggle();
    glc_poke();
    [self tick];
}

- (void)prev {
    glc_track(-1);
    glc_poke();
}

- (void)next {
    glc_track(1);
    glc_poke();
}

@end

#pragma mark - Gắn vào màn hình khoá (SBAwayView của iOS 5 / 6)

static __weak GLCPanel *gPanel;

/// Thanh «trượt để mở khoá» (TPLockBar / SBAwayLockBar…): con trực tiếp của SBAwayView có tên chứa «LockBar».
static UIView *glc_lockBar(UIView *away) {
    for (UIView *v in away.subviews) {
        if ([NSStringFromClass([v class]) rangeOfString:@"LockBar"].location != NSNotFound) return v;
    }
    return nil;
}

static void glc_layout(UIView *away) {
    GLCPanel *p = (GLCPanel *)[away viewWithTag:kPanelTag];
    if (![p isKindOfClass:[GLCPanel class]]) return;
    CGSize b = away.bounds.size;
    UIView *bar = glc_lockBar(away);
    CGFloat bottom = bar ? CGRectGetMinY(bar.frame) : b.height - 96;
    CGFloat w = MIN(b.width - 20, 300);
    CGFloat h = [p heightForWidth:w];
    p.frame = CGRectMake(floor((b.width - w) / 2), floor(bottom - h - 8), w, h);
}

static void glc_attach(UIView *away) {
    GLCPanel *p = (GLCPanel *)[away viewWithTag:kPanelTag];
    if (![p isKindOfClass:[GLCPanel class]]) p = nil;
    if (!gEnabled || !away.window) {
        if (!gEnabled) [p removeFromSuperview];
        return;
    }
    if (!p) {
        p = [[GLCPanel alloc] initWithFrame:CGRectZero];
        p.tag = kPanelTag;
        UIView *bar = glc_lockBar(away);
        if (bar) [away insertSubview:p belowSubview:bar];
        else [away addSubview:p];
        glc_log([NSString stringWithFormat:@"gắn khung vào %@ (%.0fx%.0f), thanh mở khoá: %@", NSStringFromClass([away class]),
                                           away.bounds.size.width, away.bounds.size.height,
                                           bar ? NSStringFromClass([bar class]) : @"không thấy"]);
    }
    gPanel = p;
    glc_loadState();
    glc_layout(away);
    [p refresh];
}

static void glc_didMoveToWindow(UIView *self, SEL _cmd) {
    IMP o = gOrigMove ?: class_getMethodImplementation(class_getSuperclass(gAwayClass), _cmd);
    ((void (*)(id, SEL))o)(self, _cmd);
    @try {
        glc_attach(self);
    } @catch (NSException *e) {
        glc_log([NSString stringWithFormat:@"lỗi gắn khung: %@", e]);
    }
}

static void glc_layoutSubviews(UIView *self, SEL _cmd) {
    IMP o = gOrigLayout ?: class_getMethodImplementation(class_getSuperclass(gAwayClass), _cmd);
    ((void (*)(id, SEL))o)(self, _cmd);
    if (gEnabled) glc_layout(self);
}

/// Móc một hàm của lớp `c`: lớp tự có hàm thì thay, không thì thêm hàm vào lớp (gọi hàm của lớp cha lúc chạy) — không đụng tới
/// UIView của mọi view khác.
static BOOL glc_hook(Class c, SEL sel, IMP repl, IMP *orig) {
    unsigned n = 0;
    Method *ms = class_copyMethodList(c, &n);
    Method own = NULL;
    for (unsigned i = 0; i < n; i++) {
        if (method_getName(ms[i]) == sel) {
            own = ms[i];
            break;
        }
    }
    free(ms);
    if (own) {
        *orig = method_setImplementation(own, repl);
        return YES;
    }
    Method inh = class_getInstanceMethod(c, sel);
    if (!inh) return NO;
    *orig = NULL;  // hàm của lớp cha: tra lúc gọi
    return class_addMethod(c, sel, repl, method_getTypeEncoding(inh));
}

#pragma mark - Thông báo

static void glc_prefsChanged(CFNotificationCenterRef c, void *o, CFStringRef n, const void *obj, CFDictionaryRef info) {
    glc_loadPrefs();
    glc_log([NSString stringWithFormat:@"cài đặt: bật %d, lời %d, tiến trình %d, nút %d", gEnabled, gLyrics, gProgress, gControls]);
    GLCPanel *p = gPanel;
    UIView *away = p.superview;
    if (!gEnabled) {
        [p removeFromSuperview];
        return;
    }
    if (away) {
        [p setNeedsLayout];
        glc_layout(away);
        [p refresh];
    }
}

static void glc_stateChanged(CFNotificationCenterRef c, void *o, CFStringRef n, const void *obj, CFDictionaryRef info) {
    if (!gEnabled) return;
    glc_loadState();
    GLCPanel *p = gPanel;
    if (p.superview) {
        glc_layout(p.superview);
        [p refresh];
    }
}

#ifdef DEBUG
/// Bản thử: ghi cây view của màn hình khoá («…/debug.dump») — để thử qua SSH. (Không khoá máy từ xa: -[SBAwayController lock] của
/// iOS 6 chỉ bật cờ «đã khoá» mà không hiện màn hình khoá → SpringBoard kẹt, nút Home không ăn — 17/11 trên iPhone 5.)
static void glc_dumpView(UIView *v, int depth, NSMutableString *out) {
    if (depth > 4) return;
    [out appendFormat:@"%*s%@ %@%@\n", depth * 2, "", NSStringFromClass([v class]), NSStringFromCGRect(v.frame), v.hidden ? @" (ẩn)" : @""];
    for (UIView *s in v.subviews) glc_dumpView(s, depth + 1, out);
}

static void glc_debugDump(CFNotificationCenterRef c, void *o, CFStringRef n, const void *obj, CFDictionaryRef info) {
    id m = glc_media(), aw = glc_away();
    NSMutableString *s = [NSMutableString stringWithFormat:
        @"thử: SBMediaController %d (nowPlayingApplication %d, togglePlayPause %d, changeTrack: %d), SBAwayController %d "
        @"(restartDimTimer %d, isDimmed %d), GramMusic đang phát %d, trạng thái %@\n",
        m != nil, [m respondsToSelector:@selector(nowPlayingApplication)], [m respondsToSelector:@selector(togglePlayPause)],
        [m respondsToSelector:@selector(changeTrack:)], aw != nil, [aw respondsToSelector:@selector(restartDimTimer)],
        [aw respondsToSelector:@selector(isDimmed)], glc_gramIsNowPlaying(), gState ? @"có" : @"không"];
    UIView *away = gPanel.superview;
    if (away) glc_dumpView(away, 0, s);
    else [s appendString:@"(chưa có màn hình khoá)\n"];
    glc_log(s);
}
#endif

__attribute__((constructor)) static void glc_init(void) {
    if (!objc_getClass("SpringBoard")) return;
    @autoreleasepool {
        glc_loadPrefs();
        glc_loadState();
        gAwayClass = objc_getClass("SBAwayView");
        if (!gAwayClass) {
            glc_log(@"không có SBAwayView (không phải iOS 5 / 6) — đứng ngoài");
            return;
        }
        BOOL a = glc_hook(gAwayClass, @selector(didMoveToWindow), (IMP)glc_didMoveToWindow, &gOrigMove);
        BOOL b = glc_hook(gAwayClass, @selector(layoutSubviews), (IMP)glc_layoutSubviews, &gOrigLayout);
        CFNotificationCenterRef dc = CFNotificationCenterGetDarwinNotifyCenter();
        CFNotificationCenterAddObserver(dc, NULL, glc_prefsChanged, CFSTR(kNotePrefs), NULL,
                                        CFNotificationSuspensionBehaviorDeliverImmediately);
        CFNotificationCenterAddObserver(dc, NULL, glc_stateChanged, CFSTR(kNoteState), NULL,
                                        CFNotificationSuspensionBehaviorDeliverImmediately);
#ifdef DEBUG
        CFNotificationCenterAddObserver(dc, NULL, glc_debugDump, CFSTR("vn.grammusic.lockcontrol/debug.dump"), NULL,
                                        CFNotificationSuspensionBehaviorDeliverImmediately);
#endif
        glc_log([NSString stringWithFormat:@"nạp: móc didMoveToWindow %d (%@), layoutSubviews %d (%@); bật %d", a,
                                           gOrigMove ? @"của lớp" : @"thêm", b, gOrigLayout ? @"của lớp" : @"thêm", gEnabled]);
    }
}
