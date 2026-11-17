#import "GMLockState.h"
#import "GMCore.h"
#import "GMLog.h"
#import "GMPlayer.h"
#import "GMTheme.h"
#include <notify.h>

static NSString *const kTweakPath = @"/Library/MobileSubstrate/DynamicLibraries/GramLockControl.dylib";
static NSString *const kTweakPrefs = @"/var/mobile/Library/Preferences/vn.grammusic.lockcontrol.plist";
#define kNoteState "vn.grammusic.lockcontrol/state"
#define kNoteCmd "vn.grammusic.lockcontrol/cmd."

static GMLockState *gLock;

@implementation GMLockState {
    NSString *_key;
    BOOL _playing;
    double _pos, _at;
    NSArray *_lines;  // [[giây, chữ]] lời chạy của bài đang phát
    BOOL _instrumental;  // LRCLIB: bài không lời
    BOOL _wrote;
}

+ (BOOL)tweakShowsProgress {
    static BOOL cached;
    static NSTimeInterval at;
    NSTimeInterval now = [NSDate timeIntervalSinceReferenceDate];
    if (now - at < 5) return cached;
    at = now;
    NSDictionary *d = [NSDictionary dictionaryWithContentsOfFile:kTweakPrefs];
    id on = [d objectForKey:@"enabled"], pr = [d objectForKey:@"progress"];
    cached = [[NSFileManager defaultManager] fileExistsAtPath:kTweakPath] && (!on || [on boolValue]) && (!pr || [pr boolValue]);
    return cached;
}

+ (void)start {
    if (gLock) return;
    gLock = [[GMLockState alloc] init];
    [gLock setup];
}

- (void)setup {
    // Lệnh từ màn hình khoá (tweak). Phát / dừng, bài trước / sau thường đi đường của iOS (sự kiện điều khiển từ xa) — đây là đường dự phòng.
    int tok = 0;
    GMPlayer *p = [GMPlayer shared];
    dispatch_queue_t q = dispatch_get_main_queue();
    notify_register_dispatch(kNoteCmd "toggle", &tok, q, ^(int t) { [p togglePlay]; });
    notify_register_dispatch(kNoteCmd "next", &tok, q, ^(int t) { [p next]; });
    notify_register_dispatch(kNoteCmd "prev", &tok, q, ^(int t) { [p previous]; });
    notify_register_dispatch(kNoteCmd "seek", &tok, q, ^(int t) {
        uint64_t ms = 0;
        notify_get_state(t, &ms);
        GMLog(@"màn hình khoá: tua tới %.1f s", ms / 1000.0);
        [p seek:ms / 1000.0];
    });
    if (![[NSFileManager defaultManager] fileExistsAtPath:kTweakPath]) return;  // chưa cài tweak: khỏi ghi gì
    GMLog(@"màn hình khoá: có GramLockControl");
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(changed) name:GMPlayerChangedNotification object:nil];
    [self changed];
}

/// Tweak có cần lời không (bật tweak + bật «Lời bài hát» trong Cài đặt của iPhone — mặc định bật).
- (BOOL)wantsLyrics {
    NSDictionary *d = [NSDictionary dictionaryWithContentsOfFile:kTweakPrefs];
    id on = [d objectForKey:@"enabled"], ly = [d objectForKey:@"lyrics"];
    return (!on || [on boolValue]) && (!ly || [ly boolValue]);
}

- (void)changed {
    GMPlayer *p = [GMPlayer shared];
    NSDictionary *s = p.current;
    NSString *key = [GMVal(s, @"loading") boolValue] ? nil : GMVal(s, @"key");
    BOOL playing = p.playing && !p.buffering;
    double now = [[NSDate date] timeIntervalSince1970], pos = p.position;
    double predicted = _playing ? _pos + (now - _at) : _pos;
    BOOL keyChanged = !((!key && !_key) || [key isEqual:_key]);
    // Chỉ ghi khi đổi bài / phát–dừng / vị trí lệch dự đoán (tua) — tweak tự tính vị trí theo thời gian trôi
    if (_wrote && !keyChanged && playing == _playing && fabs(pos - predicted) < 1.5) return;
    _key = key;
    _playing = playing;
    _pos = pos;
    _at = now;
    if (keyChanged) {
        _lines = nil;
        _instrumental = NO;
        if (key) [self loadLyrics:s];
    }
    [self write];
}

- (void)loadLyrics:(NSDictionary *)song {
    NSString *key = GMVal(song, @"key");
    if (![self wantsLyrics]) return;
    NSMutableDictionary *args = [NSMutableDictionary dictionaryWithObject:key forKey:@"key"];
    if ([GMVal(song, @"local") boolValue]) {
        [args setObject:GMVal(song, @"title") ?: @"" forKey:@"title"];
        [args setObject:GMVal(song, @"artist") ?: @"" forKey:@"artist"];
        [args setObject:GMVal(song, @"dur") ?: @0 forKey:@"dur"];
    }
    // Lõi chỉ hỏi LRCLIB khi anh đã bật lời bài hát trong GramMusic (LYRICS_OFF thì thôi); bản đã lưu trả ngay
    [GMCore call:@"lyrics.get" args:args done:^(id ok, NSDictionary *err) {
        if (err || ![key isEqual:_key]) return;
        if (![GMVal(ok, @"synced") boolValue]) {
            if ([GMVal(ok, @"instrumental") boolValue]) {
                _instrumental = YES;
                [self write];
            }
            return;
        }
        NSMutableArray *lines = [NSMutableArray array];
        for (NSDictionary *l in GMVal(ok, @"lines")) {
            [lines addObject:@[GMVal(l, @"t") ?: @0, GMVal(l, @"s") ?: @""]];
        }
        _lines = lines;
        [self write];
    }];
}

- (void)write {
    GMPlayer *p = [GMPlayer shared];
    NSDictionary *s = p.current;
    NSMutableDictionary *d = [NSMutableDictionary dictionary];
    [d setObject:_key ?: @"" forKey:@"key"];
    [d setObject:GMVal(s, @"title") ?: @"" forKey:@"title"];
    [d setObject:GMVal(s, @"artist") ?: @"" forKey:@"artist"];
    [d setObject:@(p.duration) forKey:@"dur"];
    [d setObject:@(_pos) forKey:@"pos"];
    [d setObject:@(_at) forKey:@"at"];
    [d setObject:@(_playing) forKey:@"playing"];
    if (_lines) [d setObject:_lines forKey:@"lines"];
    if (_instrumental) [d setObject:@YES forKey:@"instrumental"];
    [d writeToFile:[GMDataDir() stringByAppendingPathComponent:@"lockstate.plist"] atomically:YES];
    _wrote = YES;
    notify_post(kNoteState);
}

@end
