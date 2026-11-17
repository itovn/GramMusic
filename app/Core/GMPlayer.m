#import "GMPlayer.h"
#import "GMArt.h"
#import "GMLocalMusic.h"
#import "GMLockState.h"
#import "GMCore.h"
#import "GMImages.h"
#import "GMLog.h"
#import <AVFoundation/AVFoundation.h>
#import <AudioToolbox/AudioToolbox.h>
#import <MediaPlayer/MediaPlayer.h>

NSString *const GMPlayerChangedNotification = @"GMPlayerChanged";
NSString *const GMPlayerMessageNotification = @"GMPlayerMessage";

static void *kItemStatus = &kItemStatus;
static void *kItemKeepUp = &kItemKeepUp;
static void *kPlayerRate = &kPlayerRate;
static void *kPlayerItem = &kPlayerItem;

/// Bài không phát được liên tiếp quá chừng này thì dừng (khỏi nhảy qua cả hàng chờ khi mất mạng).
static const NSInteger kMaxFails = 3;

@interface GMPlayer () <AVAudioSessionDelegate>
- (void)mediaServicesReset:(NSString *)why;
@end

@implementation GMPlayer {
    AVQueuePlayer *_player;
    NSMutableArray *_keys;      // thứ tự phát
    NSMutableArray *_orig;      // thứ tự gốc (khi trộn)
    NSMutableDictionary *_info; // khoá → dict bài
    NSInteger _index;
    AVPlayerItem *_curItem, *_nextItem;
    NSString *_curKey, *_nextKey;
    NSInteger _nextIndex;
    NSMutableSet *_observed;    // item đang được KVO (bỏ đúng lúc, khỏi lỗi «not registered»)
    BOOL _wantPlay;
    BOOL _ended;                // hết hàng chờ (bấm phát = phát lại bài cuối từ đầu)
    double _resumeAt;
    NSInteger _gen;
    NSInteger _fails;
    id _timeObs;
    NSTimer *_sleepTimer;
    BOOL _sleepEnd;
    UIBackgroundTaskIdentifier _bg;
    long long _lastSave;
    UIImage *_artwork;
    MPMediaItemArtwork *_artworkItem;  // giữ lại — cập nhật màn hình khoá mỗi giây khỏi tạo lại
    NSInteger _lockSec;                // giây đã ghi lên màn hình khoá lần trước
    NSString *_artworkKey;
    BOOL _interrupted;
    long long _readyT0;  // lúc tạo item (đo thời gian tới khi sẵn sàng)
    // Không hỏi thẳng AVPlayerItem.duration / currentTime (M22): iOS 6 hỏi qua mediaserverd và chờ trả lời — mediaserverd đang kẹt
    // chờ dữ liệu (mạng chậm) thì luồng chính đứng 10 giây rồi iOS giết mediaserverd, mất tiếng
    double _curDur, _nextDur;  // độ dài bài (giây) đọc từ AVURLAsset lúc nạp
    double _obsPos;            // vị trí trình phát báo lần cuối (giây)
    long long _obsAt;          // lúc nhận vị trí đó (ms; 0 = chưa có)
    float _rate;               // tốc độ phát, cập nhật khi trình phát báo đổi
    long long _resetAt;        // lần dựng lại trình phát gần nhất sau khi mediaserverd khởi động lại
    long long _resetWindow;    // đầu khoảng 2 phút đếm số lần dựng lại
    NSInteger _resetCount;
    // Tua chờ dữ liệu (M22): chỉ ra lệnh tua cho AVPlayer khi lõi đã có dữ liệu ở đó
    NSInteger _seekGen;
    double _seekTo;            // vị trí đang chờ (-1 = không)
    BOOL _seekHold;            // mở bài ở giữa bài: chưa cho phát tới khi tua xong (khỏi phát đoạn đầu bài)
    BOOL _portRetried;         // bài này đã mở lại cổng phát + phát lại một lần sau lỗi -1004 (M23)
}

+ (instancetype)shared {
    static GMPlayer *p;
    static dispatch_once_t once;
    dispatch_once(&once, ^{ p = [[GMPlayer alloc] init]; });
    return p;
}

- (instancetype)init {
    if ((self = [super init])) {
        _keys = [NSMutableArray array];
        _info = [NSMutableDictionary dictionary];
        _observed = [NSMutableSet set];
        _nextIndex = -1;
        _index = -1;
        _bg = UIBackgroundTaskInvalid;
        _seekTo = -1;
    }
    return self;
}

#pragma mark - Phiên âm thanh

static void GMRouteChanged(void *ctx, AudioSessionPropertyID prop, UInt32 size, const void *data) {
    if (prop != kAudioSessionProperty_AudioRouteChange || !data) return;
    CFDictionaryRef d = (CFDictionaryRef)data;
    CFNumberRef r = CFDictionaryGetValue(d, CFSTR(kAudioSession_AudioRouteChangeKey_Reason));
    SInt32 reason = 0;
    if (r) CFNumberGetValue(r, kCFNumberSInt32Type, &reason);
    if (reason == kAudioSessionRouteChangeReason_OldDeviceUnavailable) {
        // Rút tai nghe: tạm dừng (như app Nhạc của iOS)
        dispatch_async(dispatch_get_main_queue(), ^{ [[GMPlayer shared] pause]; });
    }
}

/// mediaserverd (dịch vụ âm thanh của iOS) chết / khởi động lại — iOS 5 và 6 (thông báo AVAudioSessionMediaServicesWereReset chỉ
/// có từ iOS 6).
static void GMServerDied(void *ctx, AudioSessionPropertyID prop, UInt32 size, const void *data) {
    if (prop != kAudioSessionProperty_ServerDied) return;
    dispatch_async(dispatch_get_main_queue(), ^{ [[GMPlayer shared] mediaServicesReset:@"ServerDied"]; });
}

- (void)setup {
    AVAudioSession *s = [AVAudioSession sharedInstance];
    [s setCategory:AVAudioSessionCategoryPlayback error:NULL];
    s.delegate = self;  // ngắt (cuộc gọi đến) — iOS 5 chỉ có cách này (thông báo AVAudioSessionInterruption có từ iOS 6)
    AudioSessionInitialize(NULL, NULL, NULL, NULL);
    AudioSessionAddPropertyListener(kAudioSessionProperty_AudioRouteChange, GMRouteChanged, NULL);
    AudioSessionAddPropertyListener(kAudioSessionProperty_ServerDied, GMServerDied, NULL);
    [self makePlayer];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(itemEnded:)
                                                 name:AVPlayerItemDidPlayToEndTimeNotification object:nil];
    [self restoreState];
}

- (void)makePlayer {
    _player = [[AVQueuePlayer alloc] init];
    _player.actionAtItemEnd = AVPlayerActionAtItemEndAdvance;
    _rate = 0;
    [_player addObserver:self forKeyPath:@"rate" options:0 context:kPlayerRate];
    [_player addObserver:self forKeyPath:@"currentItem" options:0 context:kPlayerItem];
    __weak GMPlayer *me = self;
    // Gọi cả khi tua / bắt đầu / dừng phát — nguồn vị trí duy nhất (không hỏi currentTime)
    _timeObs = [_player addPeriodicTimeObserverForInterval:CMTimeMake(1, 1) queue:dispatch_get_main_queue() usingBlock:^(CMTime t) {
        [me timeObserved:t];
    }];
}

/// Bỏ trình phát cũ (mediaserverd đã khởi động lại — trình phát cũ không dùng được nữa). Không gọi gì sang mediaserverd.
- (void)dropPlayer {
    if (!_player) return;
    if (_timeObs) [_player removeTimeObserver:_timeObs];
    _timeObs = nil;
    [_player removeObserver:self forKeyPath:@"rate" context:kPlayerRate];
    [_player removeObserver:self forKeyPath:@"currentItem" context:kPlayerItem];
    for (AVPlayerItem *item in [_observed allObjects]) [self unwatch:item];
    _curItem = _nextItem = nil;
    _curKey = _nextKey = nil;
    _nextIndex = -1;
    _curDur = _nextDur = 0;
    _rate = 0;
    [self cancelSeek];
    _player = nil;
}

/// mediaserverd khởi động lại (bị iOS giết vì treo, hoặc tự chết): mọi item báo lỗi -11819 và trình phát cũ hỏng — dựng lại trình phát,
/// mở lại đúng bài ở đúng chỗ thay vì coi là bài lỗi rồi nhảy qua cả hàng chờ (17/11: hết nhạc, anh tưởng app tự thoát).
- (void)mediaServicesReset:(NSString *)why {
    long long now = GMNowMs();
    if (now - _resetAt < 5000) return;  // một lần cho mỗi lần khởi động lại (bài đang phát + bài nạp sẵn + ServerDied cùng báo)
    _resetAt = now;
    double at = self.position;
    BOOL play = _wantPlay;
    if (now - _resetWindow > 120000) {
        _resetWindow = now;
        _resetCount = 0;
    }
    if (++_resetCount > 3 && play) {
        // Khởi động lại liên tục: thôi tự phát tiếp (khỏi vòng lặp), chờ người dùng bấm phát
        play = NO;
        _wantPlay = NO;
        [self say:@"Dịch vụ âm thanh của iOS khởi động lại nhiều lần (mạng quá chậm?) — đã tạm dừng. Bấm phát để nghe tiếp."];
    }
    GMLog(@"phát: dịch vụ âm thanh của iOS khởi động lại (%@) — dựng lại trình phát, %@ ở %.0f giây", why, play ? @"phát tiếp" : @"giữ",
          at);
    [self dropPlayer];
    _resumeAt = at;  // vị trí hiện đúng trong lúc chờ dựng lại
    // Chờ mediaserverd lên lại rồi mới tạo trình phát mới
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1.5 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        AVAudioSession *s = [AVAudioSession sharedInstance];
        [s setCategory:AVAudioSessionCategoryPlayback error:NULL];
        AudioSessionRemovePropertyListenerWithUserData(kAudioSessionProperty_AudioRouteChange, GMRouteChanged, NULL);
        AudioSessionAddPropertyListener(kAudioSessionProperty_AudioRouteChange, GMRouteChanged, NULL);
        [self makePlayer];
        // Theo ý người dùng lúc này, không phải lúc mất dịch vụ: trong 1,5 giây chờ có thể đã bấm dừng / tua / sang bài (vị trí mới
        // ở _resumeAt — seek / loadCurrent ghi vào đó khi chưa có item)
        BOOL go = _wantPlay;
        if (go) [self holdBackground];
        if ([self currentKey]) [self loadCurrent:go at:_resumeAt];
        else [self notify];
    });
}

- (void)timeObserved:(CMTime)t {
    double s = CMTimeGetSeconds(t);
    if (_seekTo < 0 && s == s && s >= 0) {
        _obsPos = s;
        _obsAt = GMNowMs();
    }
    [self tick];
}

- (void)beginInterruption {
    _interrupted = YES;
    [self notify];
}

- (void)endInterruptionWithFlags:(NSUInteger)flags {
    _interrupted = NO;
    [[AVAudioSession sharedInstance] setActive:YES error:NULL];
    if ((flags & AVAudioSessionInterruptionFlags_ShouldResume) && _wantPlay) [_player play];
    else _wantPlay = NO;  // iOS không cho phát tiếp: coi như đã dừng (khỏi tự phát lại lúc có dữ liệu)
    [self updateNowPlaying];
    [self notify];
}

#pragma mark - Trạng thái

- (void)notify {
    [[NSNotificationCenter defaultCenter] postNotificationName:GMPlayerChangedNotification object:self];
}

- (void)say:(NSString *)text {
    [[NSNotificationCenter defaultCenter] postNotificationName:GMPlayerMessageNotification object:self userInfo:@{@"text": text}];
}

- (NSString *)currentKey {
    return _index >= 0 && _index < (NSInteger)_keys.count ? [_keys objectAtIndex:_index] : nil;
}

/// Bài đang phát; thông tin chưa lấy xong (nhảy tới bài xa trong hàng chờ dài) thì trả bản tạm chỉ có khoá — màn Đang phát / thanh
/// phát nhỏ không tưởng hàng chờ đã hết.
- (NSDictionary *)current {
    NSString *k = [self currentKey];
    if (!k) return nil;
    return [_info objectForKey:k] ?: @{@"key": k, @"title": @"…", @"loading": @YES};
}

- (NSArray *)queueKeys {
    return [_keys copy];
}

- (NSInteger)index {
    return _index;
}

- (BOOL)playing {
    // Bị app khác giành tiếng (iOS 5 / 6 nhiều khi không báo hết gián đoạn): không hiện «đang phát» khi đang im
    return _wantPlay && !_interrupted;
}

- (BOOL)buffering {
    if (!_wantPlay || !_curItem) return _wantPlay && !_curItem && [self currentKey] != nil;
    return _curItem.status != AVPlayerItemStatusReadyToPlay || _seekTo >= 0 || (_rate == 0 && !_interrupted);
}

/// Vị trí trình phát báo lần cuối (mỗi giây / khi tua / phát / dừng), cộng thời gian trôi tới giờ khi đang chạy (tối đa 1 giây).
- (double)position {
    if (!_curItem) return _resumeAt;
    if (_seekTo >= 0) return _seekTo;
    double t = _obsPos;
    if (_rate > 0 && _obsAt) t += MIN(1.0, (GMNowMs() - _obsAt) / 1000.0) * _rate;
    if (_curDur > 0 && t > _curDur) t = _curDur;
    return t == t ? MAX(0, t) : 0;
}

- (double)duration {
    if (_curItem && _curDur > 0) return _curDur;
    return [[self.current objectForKey:@"dur"] doubleValue];
}

- (NSDictionary *)infoFor:(NSString *)key {
    return [_info objectForKey:key];
}

/// Thông tin của các bài còn thiếu (lõi trả bài còn trong thư viện, bài đã mất có `missing`).
- (void)fetchInfo:(NSArray *)keys done:(void (^)(void))done {
    NSMutableArray *miss = [NSMutableArray array];
    for (NSString *k in keys) {
        if ([_info objectForKey:k]) continue;
        if ([GMLocalMusic isIPodKey:k]) {
            // Bài của app Nhạc: đọc tại chỗ (MediaPlayer), không qua lõi
            NSDictionary *s = [GMLocalMusic songForKey:k];
            [_info setObject:s ?: @{@"key": k, @"title": @"", @"missing": @YES, @"ok": @NO} forKey:k];
            continue;
        }
        [miss addObject:k];
    }
    if (!miss.count) {
        // Luôn gọi sau (như khi hỏi lõi): bên gọi là cellForRowAtIndexPath: của hàng chờ — tải lại dòng ngay trong lúc bảng đang
        // dựng ô là lỗi
        if (done) dispatch_async(dispatch_get_main_queue(), done);
        return;
    }
    [GMCore call:@"library.songs" args:@{@"keys": miss} done:^(id ok, NSDictionary *err) {
        if ([ok isKindOfClass:[NSArray class]]) {
            for (NSDictionary *s in ok) {
                NSString *k = [s objectForKey:@"key"];
                if (k) [_info setObject:s forKey:k];
            }
        }
        if (done) done();
    }];
}

- (void)fetchAround:(NSInteger)i done:(void (^)(void))done {
    NSInteger a = MAX(0, i - 2), b = MIN((NSInteger)_keys.count, i + 30);
    [self fetchInfo:[_keys subarrayWithRange:NSMakeRange(a, MAX(0, b - a))] done:done];
}

- (void)updateSong:(NSDictionary *)song {
    NSString *k = [song objectForKey:@"key"];
    if (k && [_info objectForKey:k]) {
        [_info setObject:song forKey:k];
        [self notify];
    }
}

#pragma mark - Item

- (void)watch:(AVPlayerItem *)item {
    if (!item || [_observed containsObject:item]) return;
    [item addObserver:self forKeyPath:@"status" options:0 context:kItemStatus];
    [item addObserver:self forKeyPath:@"playbackLikelyToKeepUp" options:0 context:kItemKeepUp];
    [_observed addObject:item];
}

- (void)unwatch:(AVPlayerItem *)item {
    if (!item || ![_observed containsObject:item]) return;
    [item removeObserver:self forKeyPath:@"status" context:kItemStatus];
    [item removeObserver:self forKeyPath:@"playbackLikelyToKeepUp" context:kItemKeepUp];
    [_observed removeObject:item];
}

- (void)dropNext {
    if (_nextItem) {
        if ([_player.items containsObject:_nextItem]) [_player removeItem:_nextItem];
        [self unwatch:_nextItem];
    }
    _nextItem = nil;
    _nextKey = nil;
    _nextIndex = -1;
    _nextDur = 0;
}

- (void)dropCurrent {
    [self dropNext];
    // Bỏ _curItem trước removeAllItems: KVO currentItem → nil tới ngay trong lúc gọi, currentItemChanged tưởng hết bài (bấm bài kế
    // lúc gần hết bài / bài chưa biết độ dài) rồi tự sang thêm một bài nữa
    AVPlayerItem *old = _curItem;
    _curItem = nil;
    [self unwatch:old];
    [_player removeAllItems];
    _curKey = nil;
    _curDur = 0;
    [self cancelSeek];
}

/// Tạo AVPlayerItem không chặn luồng chính. `playerItemWithURL:` đọc thử tệp ngay trên luồng gọi — WAV giải từ FLAC: AVPlayer đọc
/// cả cuối tệp, lõi phải tải + giải phần cuối bài (đo 17/11: luồng chính đứng 7,6 giây). AVURLAsset nạp ở luồng nền, xong mới tạo item.
/// `dur`: độ dài bài (giây, 0 = chưa biết) đọc từ asset đã nạp — giữ lại, không hỏi AVPlayerItem.duration (M22).
- (void)itemFor:(NSURL *)url done:(void (^)(AVPlayerItem *item, double dur))done {
    AVURLAsset *asset = [AVURLAsset URLAssetWithURL:url options:nil];
    [asset loadValuesAsynchronouslyForKeys:@[@"playable", @"duration", @"tracks"] completionHandler:^{
        double dur = 0;
        if ([asset statusOfValueForKey:@"duration" error:NULL] == AVKeyValueStatusLoaded) {
            CMTime d = asset.duration;
            if (CMTIME_IS_NUMERIC(d)) dur = CMTimeGetSeconds(d);
        }
        if (!(dur > 0)) dur = 0;
        dispatch_async(dispatch_get_main_queue(), ^{
            done([AVPlayerItem playerItemWithAsset:asset], dur);
        });
    }];
}

/// URL phát của bài: bài app Nhạc → assetURL (`{nsurl}`); còn lại hỏi lõi (`play.url` — cả tệp trên máy).
- (void)urlFor:(NSString *)key done:(void (^)(id ok, NSDictionary *err))done {
    if ([GMLocalMusic isIPodKey:key]) {
        NSURL *u = [GMLocalMusic urlForKey:key];
        if (u) done(@{@"nsurl": u, @"cached": @1}, nil);
        else done(nil, @{@"code": @"FORMAT", @"msg": @"Bài này không phát được ngoài app Nhạc (có DRM hoặc chưa tải về máy)."});
        return;
    }
    [GMCore call:@"play.url" args:@{@"key": key} done:done];
}

/// Mở bài ở `_index` (lấy URL từ lõi). `play`: phát ngay khi sẵn sàng; `at`: vị trí bắt đầu (giây).
- (void)loadCurrent:(BOOL)play at:(double)at {
    _gen++;
    NSInteger gen = _gen;
    [self dropCurrent];
    _ended = NO;
    _wantPlay = play;
    _resumeAt = at;
    NSString *key = [self currentKey];
    if (!key) {
        _wantPlay = NO;
        [self notify];
        return;
    }
    // Sang bài bằng nút màn hình khoá / tai nghe lúc app ở nền: giữ app chạy tới khi có tiếng (như advanceAfterEnd — TubeMini D34)
    if (play) [self holdBackground];
    NSDictionary *info = [_info objectForKey:key];
    if (!info) {
        [self notify];
        [self fetchAround:_index done:^{
            if (gen != _gen) return;
            if (![_info objectForKey:key]) [_info setObject:@{@"key": key, @"missing": @YES, @"ok": @NO} forKey:key];
            [self loadCurrent:play at:at];
        }];
        return;
    }
    [self updateNowPlaying];
    [self notify];
    if (![[info objectForKey:@"ok"] boolValue]) {
        [self failCurrent:[[info objectForKey:@"missing"] boolValue] ? @"Bài không còn trong thư viện — bỏ qua."
                                                                    : [NSString stringWithFormat:@"Chưa phát được định dạng %@ — bỏ qua.",
                                                                       [[info objectForKey:@"ext"] uppercaseString] ?: @"này"]];
        return;
    }
    long long ta = GMNowMs();
    [[AVAudioSession sharedInstance] setActive:YES error:NULL];
    long long t0 = GMNowMs();
    if (t0 - ta > 300) GMLog(@"phát: bật phiên âm thanh mất %lld ms", t0 - ta);
    [self reportKey:key pos:at playing:play];
    [self urlFor:key done:^(id ok, NSDictionary *err) {
        if (gen != _gen) return;
        if (err) {
            GMLog(@"phát: play.url lỗi %@", [err objectForKey:@"code"]);
            [self failCurrent:GMErrorText(err)];
            return;
        }
        long long tu = GMNowMs();
        NSURL *url = [ok objectForKey:@"nsurl"] ?: [NSURL URLWithString:[ok objectForKey:@"url"]];
        double cached = [[ok objectForKey:@"cached"] doubleValue];
        [self itemFor:url done:^(AVPlayerItem *item, double dur) {
            if (gen != _gen) return;
            long long ti = GMNowMs();
            _curItem = item;
            _curKey = key;
            _curDur = dur;
            _obsPos = at;
            _obsAt = 0;
            [self watch:item];
            [_player insertItem:item afterItem:nil];
            // Mở giữa bài: chưa phát (phát là chạy từ 0 trong lúc chờ dữ liệu ở chỗ cần tua tới) — sẵn sàng thì seekWhenReady tua rồi
            // mới phát (M22)
            if (_wantPlay && !(_resumeAt > 1)) [_player play];
            GMLog(@"phát: có URL sau %lld ms, nạp tệp %lld ms (nền), gắn item %lld ms (đệm %.0f%%)", tu - t0, ti - tu, GMNowMs() - ti,
                  cached * 100);
            _readyT0 = GMNowMs();
            if (![key hasPrefix:@"ipod:"] && ![key hasPrefix:@"file:"]) [GMCore call:@"play.played" args:@{@"key": key} done:nil];
            [self notify];
        }];
    }];
}

/// Đang ở nền mà phải mở bài mới (không có bài nạp sẵn): xin thêm giờ chạy tới khi có tiếng (iOS 6 không cho app ở nền bắt đầu
/// phát nếu đã bị treo — TubeMini D34); trả lại khi item sẵn sàng.
- (void)holdBackground {
    if (_bg == UIBackgroundTaskInvalid && [UIApplication sharedApplication].applicationState == UIApplicationStateBackground) {
        _bg = [[UIApplication sharedApplication] beginBackgroundTaskWithExpirationHandler:^{ [self endBackgroundTask]; }];
    }
}

/// Bài hiện tại không phát được: báo, sang bài sau (tối đa kMaxFails lần liền).
- (void)failCurrent:(NSString *)why {
    [self say:why];
    [self holdBackground];
    _fails++;
    if (_fails >= kMaxFails || _keys.count <= 1) {
        _fails = 0;
        _wantPlay = NO;
        [self dropCurrent];
        [self notify];
        return;
    }
    NSInteger n = [self nextIndexManual:YES];
    if (n < 0) {
        _wantPlay = NO;
        [self notify];
        return;
    }
    _index = n;
    NSInteger gen = ++_gen;
    BOOL play = _wantPlay;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.6 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        if (gen == _gen) [self loadCurrent:play at:0];
    });
}

/// Bài tiếp theo khi hết bài (lặp một bài: chính nó). -1 = hết hàng chờ.
- (NSInteger)nextIndexAuto {
    if (_repeat == GMRepeatOne) return _index;
    return [self nextIndexManual:NO];
}

/// Bài tiếp theo khi bấm «Tiếp» (lặp một bài coi như lặp cả hàng). `wrap`: hết hàng thì quay về đầu kể cả không lặp.
- (NSInteger)nextIndexManual:(BOOL)wrap {
    if (!_keys.count) return -1;
    if (_index + 1 < (NSInteger)_keys.count) return _index + 1;
    return (_repeat != GMRepeatOff || wrap) ? 0 : -1;
}

/// Nạp sẵn bài kế tiếp vào hàng của AVQueuePlayer (bài đang phát đã sẵn sàng). Bài kế không phát được (định dạng, đã mất) thì bỏ
/// qua tới bài phát được — hết bài là chuyển liền, không phải mở bài mới lúc app đang ở nền.
- (void)preloadNext {
    if (_nextItem || _nextKey || _sleepEnd || !_curItem || _curItem.status != AVPlayerItemStatusReadyToPlay) return;
    NSInteger n = [self nextIndexAuto];
    NSString *key = nil;
    NSDictionary *info = nil;
    for (NSInteger tries = 0; n >= 0 && tries < 50; tries++) {
        key = [_keys objectAtIndex:n];
        info = [_info objectForKey:key];
        if (!info || [[info objectForKey:@"ok"] boolValue]) break;
        if (n == _index) return;  // vòng hết hàng chờ mà không có bài nào phát được
        n = n + 1 < (NSInteger)_keys.count ? n + 1 : (_repeat != GMRepeatOff ? 0 : -1);
    }
    if (n < 0 || !key) return;
    NSInteger gen = _gen;
    if (!info) {
        [self fetchAround:n done:^{
            if (gen == _gen && [_info objectForKey:key]) [self preloadNext];
        }];
        return;
    }
    if (![[info objectForKey:@"ok"] boolValue]) return;
    _nextIndex = n;
    _nextKey = key;
    [self urlFor:key done:^(id ok, NSDictionary *err) {
        if (gen != _gen || ![_nextKey isEqual:key] || _nextItem) return;
        if (err) {
            _nextKey = nil;
            _nextIndex = -1;
            return;
        }
        [self itemFor:[ok objectForKey:@"nsurl"] ?: [NSURL URLWithString:[ok objectForKey:@"url"]] done:^(AVPlayerItem *item, double dur) {
            if (gen != _gen || ![_nextKey isEqual:key] || _nextItem) return;
            if (![_player canInsertItem:item afterItem:_curItem]) return;
            _nextItem = item;
            _nextDur = dur;
            [self watch:item];
            [_player insertItem:item afterItem:_curItem];
            GMLog(@"phát: đã nạp sẵn bài kế");
        }];
    }];
}

- (void)observeValueForKeyPath:(NSString *)path ofObject:(id)obj change:(NSDictionary *)change context:(void *)ctx {
    if (ctx == kItemStatus) {
        AVPlayerItem *item = obj;
        if (item.status == AVPlayerItemStatusFailed && item.error.code == -11819 /* AVErrorMediaServicesWereReset */ &&
            (item == _curItem || item == _nextItem)) {
            // mediaserverd khởi động lại: không phải bài hỏng
            GMLog(@"phát: item lỗi -11819 (dịch vụ âm thanh của iOS khởi động lại)");
            if (![NSThread isMainThread]) {
                dispatch_async(dispatch_get_main_queue(), ^{ [self mediaServicesReset:@"-11819"]; });
            } else {
                [self mediaServicesReset:@"-11819"];
            }
            return;
        }
        if (item == _curItem && item.status == AVPlayerItemStatusFailed && [item.error.domain isEqualToString:NSURLErrorDomain] &&
            item.error.code == NSURLErrorCannotConnectToHost && !_portRetried) {
            // -1004 «không kết nối được máy chủ» tới cổng phát 127.0.0.1: iOS đã thu hồi cổng lúc app nằm nền — mở lại cổng rồi phát
            // lại đúng bài, đúng chỗ (bản cũ coi là bài lỗi, nhảy hết bài này sang bài khác — M23)
            _portRetried = YES;
            double at = self.position;
            BOOL play = _wantPlay;
            NSInteger gen = _gen;
            GMLog(@"phát: không kết nối được cổng phát (-1004) — mở lại cổng, phát lại bài ở %.0f giây", at);
            [GMCore call:@"stream.revive" args:@{} done:^(id ok, NSDictionary *err) {
                if (gen == _gen) [self loadCurrent:play at:at];
            }];
            return;
        }
        if (item == _nextItem && item.status == AVPlayerItemStatusFailed) {
            // Bài nạp sẵn hỏng: bỏ khỏi hàng — hết bài hiện tại thì itemEnded mở bài kế theo cách thường
            GMLog(@"phát: bài nạp sẵn lỗi %@", item.error);
            [self dropNext];
            return;
        }
        if (item != _curItem) return;
        if (item.status == AVPlayerItemStatusReadyToPlay) {
            if (_readyT0) GMLog(@"phát: sẵn sàng sau %lld ms", GMNowMs() - _readyT0);
            _readyT0 = 0;
            _fails = 0;
            _portRetried = NO;
            if (_resumeAt > 1) {
                // Mở ở giữa bài (Nghe tiếp, dựng lại trình phát): tua khi có dữ liệu ở đó rồi mới phát
                double t = _resumeAt;
                _resumeAt = 0;
                [self seekWhenReady:t hold:YES];
            } else if (_wantPlay) {
                [_player play];
            }
            [self endBackgroundTask];
            [self preloadNext];
            [self updateNowPlaying];
        } else if (item.status == AVPlayerItemStatusFailed) {
            GMLog(@"phát: item lỗi %@", item.error);
            [self failCurrent:@"Không phát được bài này (mạng hoặc tệp hỏng)."];
        }
        [self notify];
    } else if (ctx == kItemKeepUp) {
        AVPlayerItem *item = obj;
        // Hết chờ dữ liệu (mạng chậm, vừa tua): phát tiếp nếu người dùng vẫn muốn phát
        if (item == _curItem && item.playbackLikelyToKeepUp && _wantPlay && _rate == 0 && !_interrupted && !_seekHold &&
            !(_resumeAt > 1)) {
            [_player play];
        }
        [self notify];
    } else if (ctx == kPlayerRate) {
        // Đọc ngay lúc trình phát báo đổi; chỗ khác dùng bản giữ lại
        _rate = _player.rate;
        if (_rate > 0) _interrupted = NO;  // đang có tiếng: hết gián đoạn (iOS không phải lúc nào cũng báo endInterruption)
        [self updateNowPlaying];
        [self notify];
    } else if (ctx == kPlayerItem) {
        [self currentItemChanged];
    } else {
        [super observeValueForKeyPath:path ofObject:obj change:change context:ctx];
    }
}

/// AVQueuePlayer chuyển item: sang bài đã nạp sẵn, hoặc hết hàng.
- (void)currentItemChanged {
    if (![NSThread isMainThread]) {
        dispatch_async(dispatch_get_main_queue(), ^{ [self currentItemChanged]; });
        return;
    }
    AVPlayerItem *now = _player.currentItem;
    if (now == _curItem) return;
    if (!now && _curItem && _wantPlay && !_ended && !_nextItem && _curItem.status == AVPlayerItemStatusReadyToPlay &&
        (_curDur <= 0 || self.position >= _curDur - 1.5)) {
        // Hết bài mà không có bài nạp sẵn (thông báo hết bài có thể tới sau): mở bài kế như itemEnded
        [self advanceAfterEnd];
        return;
    }
    if (now && now == _nextItem) {
        [self unwatch:_curItem];
        _curItem = _nextItem;
        _curKey = _nextKey;
        _index = _nextIndex;
        _curDur = _nextDur;
        _obsPos = 0;
        _obsAt = 0;
        [self cancelSeek];
        _nextItem = nil;
        _nextKey = nil;
        _nextIndex = -1;
        _nextDur = 0;
        _resumeAt = 0;
        _gen++;
        if (![_curKey hasPrefix:@"ipod:"] && ![_curKey hasPrefix:@"file:"]) [GMCore call:@"play.played" args:@{@"key": _curKey} done:nil];
        [self reportPace];
        [self fetchAround:_index done:nil];
        [self updateNowPlaying];
        if (_curItem.status == AVPlayerItemStatusFailed) {
            [self failCurrent:@"Không phát được bài này (mạng hoặc tệp hỏng)."];
            return;
        }
        [self preloadNext];
        [self saveState];
        [self notify];
    }
}

- (void)itemEnded:(NSNotification *)n {
    // Thông báo của AVPlayerItem có thể tới từ luồng khác
    if (![NSThread isMainThread]) {
        dispatch_async(dispatch_get_main_queue(), ^{ [self itemEnded:n]; });
        return;
    }
    if (n.object != _curItem) return;
    // Có bài nạp sẵn còn tốt: AVQueuePlayer tự sang (currentItemChanged)
    if (_nextItem && _nextItem.status != AVPlayerItemStatusFailed && _wantPlay) return;
    [self advanceAfterEnd];
}

/// Hết bài, không có bài nạp sẵn: mở bài kế (giữ app chạy ở nền tới khi có tiếng), hoặc dừng nếu hết hàng chờ.
- (void)advanceAfterEnd {
    // Hẹn giờ «Hết bài này» — ở đây chứ không chỉ ở itemEnded: KVO currentItem → nil có thể tới trước thông báo hết bài
    if (_sleepEnd) {
        _sleepEnd = NO;
        _wantPlay = NO;
        [_player pause];
    }
    [self dropNext];
    NSInteger next = [self nextIndexAuto];
    if (next < 0 || !_wantPlay) {
        // Hết hàng chờ / hẹn giờ tắt: giữ bài cuối, bấm phát là phát lại từ đầu
        _ended = YES;
        _wantPlay = NO;
        [self updateNowPlaying];
        [self notify];
        return;
    }
    [self holdBackground];
    _index = next;
    [self loadCurrent:YES at:0];
}

/// Nhịp nghe: báo lõi vị trí đang phát — lõi chỉ tải trước ~60 giây phía trước, tạm dừng thì thôi tải (DECISIONS M10).
- (void)reportKey:(NSString *)key pos:(double)pos playing:(BOOL)playing {
    if ([GMLocalMusic isIPodKey:key]) return;  // bài app Nhạc: lõi không tải gì
    if (!key) return;
    [GMCore call:@"play.pos" args:@{@"key": key, @"pos": @(pos == pos ? MAX(0, pos) : 0), @"playing": @(playing)} done:nil];
}

- (void)reportPace {
    [self reportKey:_curKey ?: [self currentKey] pos:self.position playing:_wantPlay && !_interrupted];
}

- (void)tick {
    [self reportPace];
    if (_wantPlay && GMNowMs() - _lastSave > 10000) [self saveState];
    // Máy khoá / app ở nền, không có tweak GramLockControl: thời gian chạy ở dòng thứ ba màn hình khoá (iOS 5 / 6 không có thanh tiến
    // trình cho app ngoài — M18), 10 giây một lần — mỗi lần cập nhật iOS 6 vẽ lại cả cụm tên bài (cập nhật mỗi giây: tên bài giật)
    if ([UIApplication sharedApplication].applicationState != UIApplicationStateActive && _wantPlay &&
        ![GMLockState tweakShowsProgress] && labs((NSInteger)self.position - _lockSec) >= 10) {
        [self updateNowPlaying];
    }
    [self notify];
}

#pragma mark - Điều khiển

- (void)playSongs:(NSArray *)songs at:(NSInteger)i {
    GMLog(@"phát: chọn %lu bài", (unsigned long)songs.count);
    NSMutableArray *keys = [NSMutableArray array];
    NSMutableDictionary *info = [NSMutableDictionary dictionary];
    for (NSDictionary *s in songs) {
        NSString *k = [s objectForKey:@"key"];
        if (!k) continue;
        [keys addObject:k];
        [info setObject:s forKey:k];
    }
    [self playKeys:keys at:i info:info shuffle:_shuffle];
}

- (void)playKeys:(NSArray *)keys at:(NSInteger)i info:(NSDictionary *)info shuffle:(BOOL)shuffle {
    if (!keys.count) return;
    [_info addEntriesFromDictionary:info];
    i = MAX(0, MIN(i, (NSInteger)keys.count - 1));
    _shuffle = shuffle;
    _orig = [keys mutableCopy];
    if (shuffle) {
        NSMutableArray *rest = [keys mutableCopy];
        NSString *first = [rest objectAtIndex:i];
        [rest removeObjectAtIndex:i];
        for (NSUInteger k = rest.count; k > 1; k--) [rest exchangeObjectAtIndex:k - 1 withObjectAtIndex:arc4random_uniform((u_int32_t)k)];
        [rest insertObject:first atIndex:0];
        _keys = rest;
        _index = 0;
    } else {
        _keys = [keys mutableCopy];
        _index = i;
    }
    _fails = 0;
    [self loadCurrent:YES at:0];
    [self saveState];
}

- (void)togglePlay {
    if (self.playing) [self pause];
    else [self play];
}

- (void)play {
    if (![self currentKey]) return;
    _interrupted = NO;  // người dùng bấm phát: hết gián đoạn dù iOS chưa báo
    [[AVAudioSession sharedInstance] setActive:YES error:NULL];
    if (!_curItem || _ended || _curItem.status == AVPlayerItemStatusFailed) {
        [self loadCurrent:YES at:_ended ? 0 : _resumeAt];
        return;
    }
    _wantPlay = YES;
    [self reportPace];
    [self updateNowPlaying];
    [self notify];
    // Tạm dừng lâu → app bị treo, iOS có thể đã thu hồi cổng phát (M23): kiểm (chết thì mở lại đúng số cổng) rồi mới cho AVPlayer
    // đọc tiếp. Bấm phát ở màn hình khoá / tai nghe đánh thức app ngay ở nền — không qua applicationDidBecomeActive
    NSInteger gen = _gen;
    __block BOOL went = NO;
    void (^go)(void) = ^{
        if (went) return;
        went = YES;
        // đang chờ tua (mở giữa bài, item chưa sẵn sàng / đang chờ dữ liệu): tua xong tự phát
        if (gen == _gen && _wantPlay && !_seekHold && !(_curItem && _resumeAt > 1)) [_player play];
    };
    [GMCore call:@"stream.revive" args:@{} done:^(id ok, NSDictionary *err) {
        if ([[ok objectForKey:@"revived"] boolValue]) GMLog(@"phát: cổng phát bị iOS thu hồi lúc nằm nền — đã mở lại cổng %@", [ok objectForKey:@"port"]);
        go();
    }];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1.5 * NSEC_PER_SEC)), dispatch_get_main_queue(), go);  // lõi chậm: phát luôn
}

- (void)pause {
    _wantPlay = NO;
    [_player pause];
    [self reportPace];
    [self saveState];
    [self updateNowPlaying];
    [self notify];
}

- (void)next {
    NSInteger n = [self nextIndexManual:YES];
    if (n < 0) return;
    _index = n;
    _fails = 0;
    [self loadCurrent:YES at:0];
    [self saveState];
}

- (void)previous {
    if (self.position > 3 || _keys.count <= 1) {
        [self seek:0];
        return;
    }
    _index = _index > 0 ? _index - 1 : (NSInteger)_keys.count - 1;
    _fails = 0;
    [self loadCurrent:YES at:0];
    [self saveState];
}

- (void)seek:(double)t {
    // Tua lúc app ở nền tới chỗ chưa có dữ liệu → tiếng tạm dừng → iOS 6 treo app ngay (không phát lại được): giữ app chạy tới khi
    // có tiếng (hết giờ tự trả — endBackgroundTask khi item sẵn sàng / phát tiếp)
    if (_wantPlay) [self holdBackground];
    if (_ended) {
        // Hết hàng chờ: item cũ đã rời trình phát, tua vào đó không có tác dụng — nhớ chỗ, bấm phát thì mở lại bài ở đó
        _ended = NO;
        [self dropCurrent];
        _resumeAt = MAX(0, t);
        [self notify];
        return;
    }
    if (!_curItem || _curItem.status != AVPlayerItemStatusReadyToPlay) {
        _resumeAt = t;
        return;
    }
    [self seekWhenReady:MAX(0, t) hold:_seekHold];
}

- (void)cancelSeek {
    _seekGen++;
    _seekTo = -1;
    _seekHold = NO;
}

/// Tua tới `t` khi lõi đã có dữ liệu ở đó (M22): tua tới chỗ chưa có dữ liệu thì mediaserverd chặn lúc mở đoạn đọc mới tới khi có —
/// mạng chậm quá 10 giây là iOS giết nó (mất tiếng). Trong lúc chờ: vị trí hiện chỗ mới, nhịp nghe dời tới đó (lõi tải / giải ở đó
/// trước), tiếng vẫn chạy ở chỗ cũ. `hold`: chưa cho phát tới khi tua xong (mở bài ở giữa bài).
- (void)seekWhenReady:(double)t hold:(BOOL)hold {
    NSInteger sg = ++_seekGen;
    AVPlayerItem *item = _curItem;
    NSString *key = _curKey;
    _seekTo = t;
    _seekHold = hold;
    [self reportKey:key pos:t playing:_wantPlay];
    [self updateNowPlaying];
    [self notify];
    long long t0 = GMNowMs();
    void (^go)(void) = ^{
        if (sg != _seekGen || item != _curItem) return;
        if (GMNowMs() - t0 > 1500) GMLog(@"phát: có dữ liệu ở %.0f giây sau %lld ms — tua", t, GMNowMs() - t0);
        [self seekItem:item to:t gen:sg tries:0];
    };
    if (!key || [GMLocalMusic isIPodKey:key]) {
        go();  // bài app Nhạc: tệp trên máy
        return;
    }
    [self waitData:key at:t gen:sg then:go];
}

/// Tua thật, chờ AVPlayer báo xong rồi mới bỏ «đang tua» và phát. Tua ngay lúc item vừa sẵn sàng (dữ liệu có sẵn — WAV gốc đã đệm)
/// có khi bị AVPlayer bỏ qua: trước đây phát luôn → nghe đầu bài 3–4 giây rồi mới nhảy tới chỗ cũ. Chưa xong / chưa tới chỗ → tua
/// lại (tối đa 2 lần). Trong lúc chờ: vị trí vẫn hiện `t` (_seekTo), mở giữa bài thì chưa phát (_seekHold).
- (void)seekItem:(AVPlayerItem *)item to:(double)t gen:(NSInteger)sg tries:(NSInteger)tries {
    [_player seekToTime:CMTimeMakeWithSeconds(t, 1000) completionHandler:^(BOOL finished) {
        dispatch_async(dispatch_get_main_queue(), ^{
            if (sg != _seekGen || item != _curItem) return;  // đã tua chỗ khác / đổi bài
            double now = finished ? CMTimeGetSeconds(_player.currentTime) : -1;
            if ((!finished || !(fabs(now - t) < 3)) && tries < 2) {
                GMLog(@"phát: tua tới %.0f giây chưa xong (%@) — tua lại", t, finished ? [NSString stringWithFormat:@"đang ở %.1f", now] : @"bị huỷ");
                [self seekItem:item to:t gen:sg tries:tries + 1];
                return;
            }
            _seekTo = -1;
            _seekHold = NO;
            _obsPos = t;
            _obsAt = 0;
            if (_wantPlay) [_player play];
            [self reportPace];
            [self updateNowPlaying];
            [self notify];
        });
    }];
}

/// Hỏi lõi tới khi có dữ liệu ở giây `t` (mỗi lần chờ tối đa 20 giây; lỗi hẳn thì tua luôn — AVPlayer tự báo lỗi).
- (void)waitData:(NSString *)key at:(double)t gen:(NSInteger)sg then:(void (^)(void))go {
    [GMCore call:@"play.ready" args:@{@"key": key, @"pos": @(t), @"wait": @20} done:^(id ok, NSDictionary *err) {
        if (sg != _seekGen) return;
        if (!err && ![[ok objectForKey:@"ready"] boolValue]) {
            GMLog(@"phát: chưa có dữ liệu ở %.0f giây (mạng chậm) — chờ tiếp", t);
            [self waitData:key at:t gen:sg then:go];
            return;
        }
        if (err) GMLog(@"phát: play.ready lỗi %@ — tua luôn", [err objectForKey:@"code"]);
        go();
    }];
}

- (void)jumpTo:(NSInteger)i {
    if (i < 0 || i >= (NSInteger)_keys.count) return;
    _index = i;
    _fails = 0;
    [self loadCurrent:YES at:0];
    [self saveState];
}

- (void)queueChanged {
    // Bài nạp sẵn có thể không còn là bài kế tiếp
    [self dropNext];
    [self preloadNext];
    [self saveState];
    [self notify];
}

- (void)insertNext:(NSDictionary *)song {
    NSString *k = [song objectForKey:@"key"];
    if (!k) return;
    [_info setObject:song forKey:k];
    if (![self currentKey]) {
        [self playSongs:@[song] at:0];
        return;
    }
    [_keys insertObject:k atIndex:_index + 1];
    if (_shuffle && _orig) {
        NSUInteger at = [_orig indexOfObject:[self currentKey]];
        [_orig insertObject:k atIndex:at == NSNotFound ? _orig.count : at + 1];
    }
    [self queueChanged];
}

- (void)append:(NSDictionary *)song {
    NSString *k = [song objectForKey:@"key"];
    if (!k) return;
    [_info setObject:song forKey:k];
    if (![self currentKey]) {
        [self playSongs:@[song] at:0];
        return;
    }
    [_keys addObject:k];
    if (_shuffle && _orig) [_orig addObject:k];
    [self queueChanged];
}

- (void)removeAt:(NSInteger)i {
    if (i < 0 || i >= (NSInteger)_keys.count || i == _index) return;
    NSString *k = [_keys objectAtIndex:i];
    [_keys removeObjectAtIndex:i];
    if (_shuffle && _orig) [_orig removeObject:k];
    if (i < _index) _index--;
    [self queueChanged];
}

- (void)moveFrom:(NSInteger)from to:(NSInteger)to {
    if (from < 0 || from >= (NSInteger)_keys.count || to < 0 || to >= (NSInteger)_keys.count || from == to) return;
    NSString *k = [_keys objectAtIndex:from];
    [_keys removeObjectAtIndex:from];
    [_keys insertObject:k atIndex:to];
    // Tính lại vị trí bài đang phát theo phép dời (một bài có thể có hai lần trong hàng — indexOfObject: sẽ trỏ nhầm)
    if (from == _index) _index = to;
    else if (from < _index && to >= _index) _index--;
    else if (from > _index && to <= _index) _index++;
    [self queueChanged];
}

- (void)setShuffle:(BOOL)shuffle {
    if (shuffle == _shuffle) return;
    _shuffle = shuffle;
    NSString *cur = [self currentKey];
    if (!cur) {
        [self notify];
        return;
    }
    if (shuffle) {
        _orig = [_keys mutableCopy];
        NSMutableArray *rest = [_keys mutableCopy];
        [rest removeObjectAtIndex:_index];
        for (NSUInteger k = rest.count; k > 1; k--) [rest exchangeObjectAtIndex:k - 1 withObjectAtIndex:arc4random_uniform((u_int32_t)k)];
        [rest insertObject:cur atIndex:0];
        _keys = rest;
        _index = 0;
    } else if (_orig.count) {
        _keys = _orig;
        _orig = nil;
        NSUInteger at = [_keys indexOfObject:cur];
        _index = at == NSNotFound ? 0 : at;
    }
    [self queueChanged];
}

- (void)setRepeat:(GMRepeatMode)repeat {
    _repeat = repeat;
    [self queueChanged];
}

- (void)remoteEvent:(UIEvent *)e {
    if (e.type != UIEventTypeRemoteControl) return;
    switch (e.subtype) {
        case UIEventSubtypeRemoteControlPlay: [self play]; break;
        case UIEventSubtypeRemoteControlPause: [self pause]; break;
        case UIEventSubtypeRemoteControlStop: [self pause]; break;
        case UIEventSubtypeRemoteControlTogglePlayPause: [self togglePlay]; break;
        case UIEventSubtypeRemoteControlNextTrack: [self next]; break;
        case UIEventSubtypeRemoteControlPreviousTrack: [self previous]; break;
        default: break;
    }
}

#pragma mark - Hẹn giờ tắt

- (void)setSleepMinutes:(NSInteger)minutes {
    [_sleepTimer invalidate];
    _sleepTimer = nil;
    _sleepEnd = minutes < 0;
    // «Hết bài này»: bỏ bài nạp sẵn để hết bài là dừng hẳn (AVQueuePlayer không tự sang bài kế); tắt hẹn thì nạp lại
    if (_sleepEnd) [self dropNext];
    else [self preloadNext];
    if (minutes > 0) {
        _sleepTimer = [NSTimer scheduledTimerWithTimeInterval:minutes * 60.0 target:self selector:@selector(sleepFired)
                                                     userInfo:nil repeats:NO];
    }
    [self notify];
}

- (void)sleepFired {
    _sleepTimer = nil;
    GMLog(@"hẹn giờ tắt: dừng phát");
    [self pause];
}

- (NSTimeInterval)sleepRemaining {
    if (_sleepEnd) return -1;
    if (!_sleepTimer) return 0;
    return MAX(1, [_sleepTimer.fireDate timeIntervalSinceNow]);
}

#pragma mark - Màn hình khoá

- (void)updateNowPlaying {
    NSDictionary *s = self.current;
    Class center = NSClassFromString(@"MPNowPlayingInfoCenter");
    if (!center) return;
    if (!s) {
        [MPNowPlayingInfoCenter defaultCenter].nowPlayingInfo = nil;
        return;
    }
    NSMutableDictionary *d = [NSMutableDictionary dictionary];
    [d setObject:[s objectForKey:@"title"] ?: @"" forKey:MPMediaItemPropertyTitle];
    NSString *artist = [s objectForKey:@"artist"];
    if ([artist isKindOfClass:[NSString class]] && artist.length) [d setObject:artist forKey:MPMediaItemPropertyArtist];
    NSString *src = [s objectForKey:@"src"];
    if (![src isKindOfClass:[NSString class]]) src = @"";
    // Dòng thứ ba trên màn hình khoá: «1:23 / 4:05 · nguồn» — màn hình khoá iOS 5 / 6 không vẽ thanh tiến trình cho app ngoài
    // (chỉ app Nhạc có; thanh cho mọi app từ iOS 7) nên ghi thời gian thành chữ, tick cập nhật mỗi giây khi máy khoá
    double pos = self.position, dur = self.duration;
    _lockSec = (NSInteger)pos;
    NSString *times = dur > 0 ? [NSString stringWithFormat:@"%@ / %@", GMDuration(pos), GMDuration(dur)] : GMDuration(pos);
    NSString *line3 = src.length ? [NSString stringWithFormat:@"%@ · %@", times, src] : times;
    // Có tweak hiện thanh tiến trình (M20): dòng thứ ba chỉ là nguồn như thường
    if ([GMLockState tweakShowsProgress]) line3 = src;
    if (line3.length) [d setObject:line3 forKey:MPMediaItemPropertyAlbumTitle];
    [d setObject:@(dur) forKey:MPMediaItemPropertyPlaybackDuration];
    [d setObject:@(pos) forKey:MPNowPlayingInfoPropertyElapsedPlaybackTime];
    [d setObject:@(_rate) forKey:MPNowPlayingInfoPropertyPlaybackRate];
    NSString *key = [s objectForKey:@"key"];
    BOOL fetch = NO;
    if (_artwork && [_artworkKey isEqual:key]) {
        if (!_artworkItem) _artworkItem = [[MPMediaItemArtwork alloc] initWithImage:_artwork];
        [d setObject:_artworkItem forKey:MPMediaItemPropertyArtwork];
    } else if (![_artworkKey isEqual:key] && ![[s objectForKey:@"loading"] boolValue]) {
        _artworkKey = key;
        _artwork = nil;
        _artworkItem = nil;
        fetch = YES;
    }
    // Đặt trước khi hỏi ảnh bìa: ảnh có sẵn trong bộ đệm thì done chạy ngay và gọi lại hàm này (bản có ảnh không bị ghi đè)
    [MPNowPlayingInfoCenter defaultCenter].nowPlayingInfo = d;
    if (fetch) {
        [GMImages cover:s size:300 done:^(UIImage *img) {
            if (![_artworkKey isEqual:key]) return;
            _artwork = img;
            _artworkItem = nil;
            [self updateNowPlaying];
        }];
    }
}

#pragma mark - Lưu / khôi phục («Nghe tiếp»)

- (void)saveState {
    _lastSave = GMNowMs();
    NSString *cur = [self currentKey];
    NSMutableDictionary *st = [NSMutableDictionary dictionary];
    if (cur) {
        // Giữ tối đa 3000 bài quanh bài đang phát
        NSInteger a = MAX(0, _index - 500), b = MIN((NSInteger)_keys.count, a + 3000);
        [st setObject:[_keys subarrayWithRange:NSMakeRange(a, b - a)] forKey:@"keys"];
        [st setObject:@(_index - a) forKey:@"index"];
        [st setObject:@(self.position) forKey:@"pos"];
        NSDictionary *info = [_info objectForKey:cur];
        if (info) {
            // NSUserDefaults không nhận NSNull (JSON null) → bỏ các khoá null
            NSMutableDictionary *clean = [NSMutableDictionary dictionary];
            [info enumerateKeysAndObjectsUsingBlock:^(id k, id v, BOOL *stop) {
                if (v != [NSNull null]) [clean setObject:v forKey:k];
            }];
            [st setObject:clean forKey:@"info"];
        }
    }
    [st setObject:@(_shuffle) forKey:@"shuffle"];
    [st setObject:@(_repeat) forKey:@"repeat"];
    [[NSUserDefaults standardUserDefaults] setObject:st forKey:@"player.state"];
}

- (void)restoreState {
    NSDictionary *st = [[NSUserDefaults standardUserDefaults] objectForKey:@"player.state"];
    if (![st isKindOfClass:[NSDictionary class]]) return;
    _shuffle = [[st objectForKey:@"shuffle"] boolValue];
    _repeat = [[st objectForKey:@"repeat"] integerValue];
    NSArray *keys = [st objectForKey:@"keys"];
    if (![keys isKindOfClass:[NSArray class]] || !keys.count) return;
    _keys = [keys mutableCopy];
    _index = MIN(MAX(0, [[st objectForKey:@"index"] integerValue]), (NSInteger)keys.count - 1);
    _resumeAt = [[st objectForKey:@"pos"] doubleValue];
    NSDictionary *info = [st objectForKey:@"info"];
    if ([info isKindOfClass:[NSDictionary class]]) [_info setObject:info forKey:[self currentKey]];
    // Cập nhật thông tin (tên / đã tải có thể đã đổi) — không phát
    [self fetchAround:_index done:^{ [self notify]; }];
    [self notify];
}

#pragma mark - Nền

- (void)endBackgroundTask {
    if (_bg != UIBackgroundTaskInvalid) {
        [[UIApplication sharedApplication] endBackgroundTask:_bg];
        _bg = UIBackgroundTaskInvalid;
    }
}

- (void)didEnterBackground {
    [self saveState];
    [self updateNowPlaying];  // thời gian trên màn hình khoá đúng ngay lúc khoá máy
    // Bài đang mở dở (chưa có tiếng) lúc xuống nền: iOS treo app trước khi tiếng chạy → xin thêm giờ tới khi phát được
    if (_wantPlay && self.buffering && _bg == UIBackgroundTaskInvalid) {
        _bg = [[UIApplication sharedApplication] beginBackgroundTaskWithExpirationHandler:^{ [self endBackgroundTask]; }];
    }
}

- (void)willEnterForeground {
    [self endBackgroundTask];
    // Item hỏng / kẹt trong lúc ở nền → mở lại đúng chỗ
    if (_wantPlay && _curItem && _curItem.status == AVPlayerItemStatusFailed) [self loadCurrent:YES at:self.position];
    [self notify];
}

@end
