#import "GMNowPlayingViewController.h"
#import "GMArt.h"
#import "GMCore.h"
#import "GMImages.h"
#import "GMLyricsView.h"
#import "GMPlayer.h"
#import "GMSongActions.h"
#import "GMSongCell.h"
#import "GMTheme.h"
#import <MediaPlayer/MediaPlayer.h>
#import <QuartzCore/QuartzCore.h>

static UIButton *GMGlyphButton(NSString *glyph, CGFloat size, id target, SEL action) {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeCustom];
    [b setImage:[GMArt glyph:glyph size:size color:[GMTheme text]] forState:UIControlStateNormal];
    b.showsTouchWhenHighlighted = YES;
    [b addTarget:target action:action forControlEvents:UIControlEventTouchUpInside];
    return b;
}

static UIBarButtonItem *GMGlyphItem(NSString *glyph, id target, SEL action) {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeCustom];
    [b setImage:[GMArt glyph:glyph size:22 color:[GMTheme barText]] forState:UIControlStateNormal];
    b.frame = CGRectMake(0, 0, 40, 30);
    b.showsTouchWhenHighlighted = YES;
    [b addTarget:target action:action forControlEvents:UIControlEventTouchUpInside];
    return [[UIBarButtonItem alloc] initWithCustomView:b];
}

@implementation GMNowPlayingViewController {
    UIImageView *_cover;
    UILabel *_title, *_artist, *_src, *_elapsed, *_remain, *_sleep;
    UISlider *_slider;
    UIButton *_shuffle, *_prev, *_play, *_next, *_repeat;
    UIButton *_fav, *_add, *_dl, *_ly, *_timer;
    GMLyricsView *_lyrics;  // lời bài hát đè lên ảnh bìa
    NSTimer *_lyTimer;      // 4 lần / giây khi đang xem lời (tô dòng đang hát)
    UIActivityIndicatorView *_spin;
    MPVolumeView *_volume;
    NSString *_key;
    BOOL _dragging;
    UIView *_chips;        // ô thông số tệp: định dạng · tần số · bit · bitrate · cỡ
    NSString *_infoKey;
    NSInteger _infoTries;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Đang phát";
    self.view.backgroundColor = [GMTheme background];
    [self.view addSubview:[[GMWallpaperView alloc] initWithFrame:self.view.bounds]];
    self.navigationItem.leftBarButtonItem = GMGlyphItem(@"close", self, @selector(close));
    self.navigationItem.rightBarButtonItem = GMGlyphItem(@"queue", self, @selector(openQueue));

    _cover = [[UIImageView alloc] init];
    _cover.layer.cornerRadius = 8;
    _cover.clipsToBounds = YES;
    _cover.contentMode = UIViewContentModeScaleAspectFill;
    [self.view addSubview:_cover];
    UISwipeGestureRecognizer *l = [[UISwipeGestureRecognizer alloc] initWithTarget:[GMPlayer shared] action:@selector(next)];
    l.direction = UISwipeGestureRecognizerDirectionLeft;
    UISwipeGestureRecognizer *r = [[UISwipeGestureRecognizer alloc] initWithTarget:[GMPlayer shared] action:@selector(previous)];
    r.direction = UISwipeGestureRecognizerDirectionRight;
    _cover.userInteractionEnabled = YES;
    [_cover addGestureRecognizer:l];
    [_cover addGestureRecognizer:r];
    [_cover addGestureRecognizer:[[UITapGestureRecognizer alloc] initWithTarget:self action:@selector(toggleLyrics)]];
    _lyrics = [[GMLyricsView alloc] initWithFrame:CGRectZero];
    _lyrics.hidden = ![[NSUserDefaults standardUserDefaults] boolForKey:@"GMShowLyrics"];
    for (NSNumber *dir in @[@(UISwipeGestureRecognizerDirectionLeft), @(UISwipeGestureRecognizerDirectionRight)]) {
        BOOL left = dir.integerValue == UISwipeGestureRecognizerDirectionLeft;
        UISwipeGestureRecognizer *g = [[UISwipeGestureRecognizer alloc] initWithTarget:[GMPlayer shared]
                                                                                action:left ? @selector(next) : @selector(previous)];
        g.direction = dir.integerValue;
        [_lyrics addGestureRecognizer:g];
    }
    [self.view addSubview:_lyrics];

    _title = [self label:[UIFont boldSystemFontOfSize:18] color:[GMTheme text]];
    _artist = [self label:[UIFont systemFontOfSize:15] color:[GMTheme secondaryText]];
    _src = [self label:[UIFont systemFontOfSize:12] color:[GMTheme secondaryText]];
    _elapsed = [self label:[UIFont systemFontOfSize:11] color:[GMTheme secondaryText]];
    _remain = [self label:[UIFont systemFontOfSize:11] color:[GMTheme secondaryText]];
    _elapsed.textAlignment = 0;
    _remain.textAlignment = GMAlignRight;
    _sleep = [self label:[UIFont systemFontOfSize:11] color:[GMTheme accent]];
    _chips = [[UIView alloc] init];
    _chips.backgroundColor = [UIColor clearColor];
    [self.view addSubview:_chips];

    _slider = [[UISlider alloc] init];
    [_slider setThumbImage:[GMArt sliderThumb] forState:UIControlStateNormal];
    [_slider setMinimumTrackImage:[GMArt sliderTrack:YES] forState:UIControlStateNormal];
    [_slider setMaximumTrackImage:[GMArt sliderTrack:NO] forState:UIControlStateNormal];
    [_slider addTarget:self action:@selector(sliding) forControlEvents:UIControlEventValueChanged];
    [_slider addTarget:self action:@selector(slideStart) forControlEvents:UIControlEventTouchDown];
    [_slider addTarget:self action:@selector(slideEnd) forControlEvents:UIControlEventTouchUpInside | UIControlEventTouchUpOutside |
                                                                       UIControlEventTouchCancel];
    [self.view addSubview:_slider];

    GMPlayer *p = [GMPlayer shared];
    _shuffle = GMGlyphButton(@"shuffle", 22, self, @selector(toggleShuffle));
    _prev = GMGlyphButton(@"prev", 30, p, @selector(previous));
    _play = GMGlyphButton(@"play", 44, p, @selector(togglePlay));
    _next = GMGlyphButton(@"next", 30, p, @selector(next));
    _repeat = GMGlyphButton(@"repeat", 22, self, @selector(cycleRepeat));
    _fav = GMGlyphButton(@"heart", 22, self, @selector(toggleFav));
    _add = GMGlyphButton(@"plus", 22, self, @selector(addToPlaylist));
    _dl = GMGlyphButton(@"download", 22, self, @selector(download));
    _ly = GMGlyphButton(@"lyrics", 22, self, @selector(toggleLyrics));
    _timer = GMGlyphButton(@"timer", 22, self, @selector(sleepMenu));
    for (UIButton *b in @[_shuffle, _prev, _play, _next, _repeat, _fav, _add, _dl, _ly, _timer]) [self.view addSubview:b];
    _spin = [[UIActivityIndicatorView alloc] initWithActivityIndicatorStyle:
             [GMTheme dark] ? UIActivityIndicatorViewStyleWhiteLarge : UIActivityIndicatorViewStyleGray];
    _spin.hidesWhenStopped = YES;
    [self.view addSubview:_spin];

    _volume = [[MPVolumeView alloc] init];
    _volume.showsRouteButton = YES;  // AirPlay (iOS 4.2+)
    [self.view addSubview:_volume];

    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(refresh) name:GMPlayerChangedNotification object:nil];
    [self refresh];
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    [self lyricsTimer:!_lyrics.hidden];
}

- (void)viewWillDisappear:(BOOL)animated {
    [super viewWillDisappear:animated];
    [self lyricsTimer:NO];  // NSTimer giữ màn này — tắt khi rời màn
}

#pragma mark - Lời bài hát

- (void)lyricsTimer:(BOOL)on {
    if (on && !_lyTimer) {
        _lyTimer = [NSTimer scheduledTimerWithTimeInterval:0.25 target:self selector:@selector(lyricsTick) userInfo:nil repeats:YES];
    } else if (!on && _lyTimer) {
        [_lyTimer invalidate];
        _lyTimer = nil;
    }
}

- (void)lyricsTick {
    GMPlayer *p = [GMPlayer shared];
    if (p.playing && !_dragging) [_lyrics setTime:p.position];
}

- (void)toggleLyrics {
    _lyrics.hidden = !_lyrics.hidden;
    [[NSUserDefaults standardUserDefaults] setBool:!_lyrics.hidden forKey:@"GMShowLyrics"];
    if (!_lyrics.hidden) {
        [_lyrics showSong:[GMPlayer shared].current];
        [_lyrics setTime:[GMPlayer shared].position];
    }
    [self lyricsTimer:!_lyrics.hidden];
    [self refresh];
}

- (UILabel *)label:(UIFont *)f color:(UIColor *)c {
    UILabel *l = [[UILabel alloc] init];
    l.font = f;
    l.textColor = c;
    l.backgroundColor = [UIColor clearColor];
    l.textAlignment = GMAlignCenter;
    [self.view addSubview:l];
    return l;
}

- (void)viewWillLayoutSubviews {
    [super viewWillLayoutSubviews];
    CGFloat w = self.view.bounds.size.width, h = self.view.bounds.size.height;
    // Phần dưới (chữ + thanh thời gian + nút + âm lượng) cần ~250 điểm; ảnh bìa lấy phần còn lại
    CGFloat side = MIN(MIN(w - 60, h - 292), 420);
    side = MAX(side, 120);
    CGFloat y = MAX(12, (h - 292 - side) / 2);
    _cover.frame = CGRectMake(floor((w - side) / 2), y, side, side);
    _lyrics.frame = _cover.frame;
    y += side + 10;
    CGFloat cw = MIN(w - 40, 440), cx = floor((w - cw) / 2);
    _title.frame = CGRectMake(cx, y, cw, 22);
    _artist.frame = CGRectMake(cx, y + 22, cw, 19);
    _src.frame = CGRectMake(cx, y + 41, cw, 15);
    y += 60;
    _chips.frame = CGRectMake(cx, y, cw, 22);
    [self layoutChips];
    y += 30;
    _slider.frame = CGRectMake(cx, y, cw, 24);
    _elapsed.frame = CGRectMake(cx, y + 22, 60, 14);
    _remain.frame = CGRectMake(cx + cw - 60, y + 22, 60, 14);
    _sleep.frame = CGRectMake(cx + 60, y + 22, cw - 120, 14);
    y += 42;
    CGFloat mid = w / 2;
    _play.frame = CGRectMake(mid - 32, y, 64, 56);
    _spin.center = _play.center;
    _prev.frame = CGRectMake(mid - 32 - 70, y + 6, 56, 44);
    _next.frame = CGRectMake(mid + 32 + 14, y + 6, 56, 44);
    _shuffle.frame = CGRectMake(cx - 6, y + 8, 44, 40);
    _repeat.frame = CGRectMake(cx + cw - 38, y + 8, 44, 40);
    y += 60;
    CGFloat step = cw / 5;
    NSArray *row = @[_fav, _add, _dl, _ly, _timer];
    for (NSUInteger i = 0; i < row.count; i++) [[row objectAtIndex:i] setFrame:CGRectMake(cx + step * i + (step - 44) / 2, y, 44, 36)];
    y += 42;
    _volume.frame = CGRectMake(cx, MIN(y, h - 28), cw, 24);
}

- (void)refresh {
    GMPlayer *p = [GMPlayer shared];
    NSDictionary *s = p.current;
    if (!s) {
        [self close];
        return;
    }
    _title.text = GMVal(s, @"title");
    NSString *artist = GMVal(s, @"artist");
    _artist.text = artist.length ? artist : @"Không rõ ca sĩ";
    NSString *src = GMVal(s, @"src");
    NSString *pos = p.queueKeys.count > 1 ? [NSString stringWithFormat:@"%ld / %lu", (long)p.index + 1, (unsigned long)p.queueKeys.count] : @"";
    _src.text = src.length ? (pos.length ? [NSString stringWithFormat:@"%@ · %@", src, pos] : src) : pos;
    NSString *key = GMVal(s, @"key");
    if (![key isEqual:_key]) {
        // Thông tin bài chưa về (bản tạm): chưa ghi nhớ khoá — về rồi mới hỏi ảnh bìa thật
        _key = [GMVal(s, @"loading") boolValue] ? nil : key;
        CGFloat side = MAX(_cover.bounds.size.width, 280);
        _cover.image = [GMImages cachedCover:key size:side] ?: [GMArt placeholderFor:key size:side];
        [GMImages cover:s size:side done:^(UIImage *img) {
            if ([_key isEqual:key]) _cover.image = img;
        }];
    }
    if (!_lyrics.hidden) {
        [_lyrics showSong:s];
        if (!_dragging) [_lyrics setTime:p.position];
    }
    [_ly setImage:[GMArt glyph:@"lyrics" size:22 color:_lyrics.hidden ? [GMTheme text] : [GMTheme accent]] forState:UIControlStateNormal];
    if (key && ![GMVal(s, @"loading") boolValue] && ![key isEqual:_infoKey]) {
        _infoKey = key;
        _infoTries = 0;
        for (UIView *v in _chips.subviews) [v removeFromSuperview];
        [self loadInfo];
    }
    double d = p.duration, t = p.position;
    if (!_dragging) {
        _slider.maximumValue = MAX(1, d);
        _slider.value = MIN(t, MAX(1, d));
        _elapsed.text = GMDuration(t);
        _remain.text = d > 0 ? [@"-" stringByAppendingString:GMDuration(MAX(0, d - t))] : @"";
    }
    BOOL busy = p.playing && p.buffering;
    if (busy) [_spin startAnimating];
    else [_spin stopAnimating];
    _play.hidden = busy;
    [_play setImage:[GMArt glyph:p.playing ? @"pause" : @"play" size:44 color:[GMTheme text]] forState:UIControlStateNormal];
    UIColor *on = [GMTheme accent], *off = [GMTheme secondaryText];
    [_shuffle setImage:[GMArt glyph:@"shuffle" size:22 color:p.shuffle ? on : off] forState:UIControlStateNormal];
    [_repeat setImage:[GMArt glyph:p.repeat == GMRepeatOne ? @"repeat1" : @"repeat" size:22 color:p.repeat != GMRepeatOff ? on : off]
             forState:UIControlStateNormal];
    // Bài trên máy: Yêu thích / Playlist / Tải về chỉ dành cho bài trên Telegram
    BOOL local = [GMVal(s, @"local") boolValue];
    for (UIButton *b in @[_fav, _add, _dl]) {
        b.enabled = !local;
        b.alpha = local ? 0.3 : 1;
    }
    BOOL fav = [GMVal(s, @"fav") boolValue];
    [_fav setImage:[GMArt glyph:fav ? @"heartFill" : @"heart" size:22 color:fav ? [UIColor colorWithRed:0.93 green:0.3 blue:0.4 alpha:1] : [GMTheme text]]
          forState:UIControlStateNormal];
    NSString *dl = GMVal(s, @"dl");
    BOOL have = [dl isEqual:@"offline"];
    [_dl setImage:[GMArt glyph:have ? @"downloaded" : @"download" size:22 color:have ? [GMTheme checkColor] : [GMTheme text]]
         forState:UIControlStateNormal];
    NSTimeInterval sl = p.sleepRemaining;
    _sleep.text = sl < 0 ? @"Tắt khi hết bài" : (sl > 0 ? [NSString stringWithFormat:@"Tắt sau %@", GMDuration(sl)] : @"");
    [_timer setImage:[GMArt glyph:@"timer" size:22 color:sl != 0 ? on : [GMTheme text]] forState:UIControlStateNormal];
}

#pragma mark - Thông số tệp

/// Hỏi lõi thông số tệp (đọc phần đầu tệp đã tải); khúc đầu chưa có thì hỏi lại sau 2 giây (tối đa 5 lần).
- (void)loadInfo {
    NSString *key = _infoKey;
    if (!key) return;
    if ([key hasPrefix:@"ipod:"]) {
        // Bài app Nhạc: chỉ biết định dạng theo đuôi tệp
        NSString *ext = [GMVal([GMPlayer shared].current, @"ext") uppercaseString];
        if (ext.length) [self showInfo:@{@"format": ext, @"lossless": @NO}];
        return;
    }
    [GMCore call:@"play.info" args:@{@"key": key} done:^(id ok, NSDictionary *err) {
        if (![key isEqual:_infoKey] || err) return;
        [self showInfo:ok];
        if (![GMVal(ok, @"ready") boolValue] && ++_infoTries < 5) {
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
                if ([key isEqual:_infoKey]) [self loadInfo];
            });
        }
    }];
}

static NSString *GMRateText(NSNumber *rate) {
    double k = rate.doubleValue / 1000.0;
    return fmod(k, 1.0) == 0 ? [NSString stringWithFormat:@"%.0fkHz", k] : [NSString stringWithFormat:@"%.1fkHz", k];
}

- (void)showInfo:(NSDictionary *)i {
    NSMutableArray *texts = [NSMutableArray array];
    NSString *fmt = GMVal(i, @"format");
    if (fmt.length) [texts addObject:fmt];
    NSNumber *rate = GMVal(i, @"rate");
    if (rate.intValue > 0) [texts addObject:GMRateText(rate)];
    NSNumber *bits = GMVal(i, @"bits");
    BOOL lossless = [GMVal(i, @"lossless") boolValue];
    if (bits.intValue > 0 && (lossless || bits.intValue != 16)) [texts addObject:[NSString stringWithFormat:@"%d-bit", bits.intValue]];
    NSNumber *kbps = GMVal(i, @"kbps");
    if (kbps.intValue > 0) {
        [texts addObject:[NSString stringWithFormat:@"%@%d kbps", [GMVal(i, @"vbr") boolValue] ? @"~" : @"", kbps.intValue]];
    }
    NSNumber *size = GMVal(i, @"size");
    if (size.longLongValue > 0) [texts addObject:[GMTheme sizeText:size.doubleValue]];
    for (UIView *v in _chips.subviews) [v removeFromSuperview];
    for (NSUInteger n = 0; n < texts.count; n++) {
        UILabel *l = [[UILabel alloc] init];
        l.text = [texts objectAtIndex:n];
        l.textAlignment = GMAlignCenter;
        // Định dạng lossless (FLAC, ALAC, WAV) tô màu chủ đạo
        UIColor *c = (n == 0 && lossless) ? [GMTheme accent] : [GMTheme text];
        l.textColor = c;
        l.backgroundColor = [c colorWithAlphaComponent:0.10];
        l.layer.borderColor = [c colorWithAlphaComponent:0.35].CGColor;
        l.layer.borderWidth = 1;
        l.layer.cornerRadius = 11;
        l.clipsToBounds = YES;
        [_chips addSubview:l];
    }
    [self layoutChips];
}

/// Xếp các ô giữa hàng; không đủ chỗ (iPhone, 5 ô) thì thu chữ nhỏ lại.
- (void)layoutChips {
    NSArray *ls = _chips.subviews;
    if (!ls.count) return;
    CGFloat avail = _chips.bounds.size.width, gap = 6;
    for (CGFloat fs = 12; fs >= 9; fs -= 1) {
        CGFloat pad = fs >= 11 ? 18 : 12, total = gap * (ls.count - 1);
        UIFont *f = [UIFont boldSystemFontOfSize:fs];
        for (UILabel *l in ls) total += ceil([l.text sizeWithFont:f].width) + pad;
        if (total <= avail || fs <= 9) {
            CGFloat x = floor(MAX(0, (avail - total) / 2));
            for (UILabel *l in ls) {
                l.font = f;
                CGFloat wl = ceil([l.text sizeWithFont:f].width) + pad;
                l.frame = CGRectMake(x, 0, wl, 22);
                x += wl + gap;
            }
            return;
        }
    }
}

- (void)close {
    if (self.presentingViewController) [self dismissViewControllerAnimated:YES completion:nil];
}

- (void)openQueue {
    [self.navigationController pushViewController:[[GMQueueViewController alloc] initWithStyle:UITableViewStylePlain] animated:YES];
}

- (void)slideStart {
    _dragging = YES;
}

- (void)sliding {
    double d = [GMPlayer shared].duration;
    _elapsed.text = GMDuration(_slider.value);
    _remain.text = d > 0 ? [@"-" stringByAppendingString:GMDuration(MAX(0, d - _slider.value))] : @"";
}

- (void)slideEnd {
    _dragging = NO;
    [[GMPlayer shared] seek:_slider.value];
}

- (void)toggleShuffle {
    GMPlayer *p = [GMPlayer shared];
    p.shuffle = !p.shuffle;
    [GMTheme toast:p.shuffle ? @"Phát ngẫu nhiên" : @"Phát theo thứ tự" in:self.view];
}

- (void)cycleRepeat {
    GMPlayer *p = [GMPlayer shared];
    p.repeat = (p.repeat + 1) % 3;
    [GMTheme toast:p.repeat == GMRepeatOne ? @"Lặp một bài" : p.repeat == GMRepeatAll ? @"Lặp cả hàng chờ" : @"Không lặp" in:self.view];
}

- (void)toggleFav {
    NSDictionary *s = [GMPlayer shared].current;
    NSString *key = GMVal(s, @"key");
    if (!key || [GMVal(s, @"missing") boolValue]) return;
    BOOL fav = [GMVal(s, @"fav") boolValue];
    [GMCore call:@"fav.set" args:@{@"key": key, @"on": @(!fav)} done:^(id ok, NSDictionary *err) {
        if (err) {
            [GMTheme toast:GMErrorText(err) in:self.view];
            return;
        }
        NSMutableDictionary *m = [s mutableCopy];
        [m setObject:@(!fav) forKey:@"fav"];
        [[GMPlayer shared] updateSong:m];
        [GMTheme toast:fav ? @"Đã bỏ khỏi Yêu thích" : @"Đã thêm vào Yêu thích" in:self.view];
    }];
}

- (void)addToPlaylist {
    NSString *key = GMVal([GMPlayer shared].current, @"key");
    if (key) [GMSongActions addKeys:@[key] toPlaylistFrom:self];
}

- (void)download {
    NSDictionary *s = [GMPlayer shared].current;
    NSString *key = GMVal(s, @"key");
    if (!key) return;
    NSString *dl = GMVal(s, @"dl");
    if ([dl isEqual:@"offline"] || [dl isEqual:@"queued"]) {
        [GMTheme toast:@"Bài đã có trong «Đã tải»." in:self.view];
        return;
    }
    [GMCore call:@"dl.add" args:@{@"keys": @[key]} done:^(id ok, NSDictionary *err) {
        if (err) {
            [GMTheme toast:GMErrorText(err) in:self.view];
            return;
        }
        NSMutableDictionary *m = [s mutableCopy];
        [m setObject:@"queued" forKey:@"dl"];
        [[GMPlayer shared] updateSong:m];
        [GMTheme toast:@"Đang tải về máy…" in:self.view];
    }];
}

- (void)sleepMenu {
    UIActionSheet *a = [[UIActionSheet alloc] initWithTitle:@"Hẹn giờ tắt nhạc" delegate:self cancelButtonTitle:nil
                                     destructiveButtonTitle:nil otherButtonTitles:@"Tắt hẹn giờ", @"15 phút", @"30 phút", @"45 phút",
                                                                                  @"60 phút", @"Hết bài này", nil];
    a.cancelButtonIndex = [a addButtonWithTitle:@"Huỷ"];
    if (UI_USER_INTERFACE_IDIOM() == UIUserInterfaceIdiomPad) [a showFromRect:_timer.bounds inView:_timer animated:YES];
    else [a showInView:self.view];
}

- (void)actionSheet:(UIActionSheet *)sheet clickedButtonAtIndex:(NSInteger)i {
    if (i == sheet.cancelButtonIndex || i < 0) return;
    NSInteger mins[] = {0, 15, 30, 45, 60, -1};
    if (i < 6) [[GMPlayer shared] setSleepMinutes:mins[i]];
}

@end

#pragma mark - Hàng chờ

@implementation GMQueueViewController {
    NSArray *_keys;
    NSInteger _index;
    BOOL _mutating;  // đang tự sửa hàng chờ (xoá / dời) — thông báo của trình phát lúc đó không tải lại bảng
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Hàng chờ";
    self.tableView.rowHeight = [GMSongCell height];
    self.navigationItem.rightBarButtonItem = self.editButtonItem;
    CGFloat w = self.tableView.bounds.size.width;
    UIView *h = [[UIView alloc] initWithFrame:CGRectMake(0, 0, w, 48)];
    UIButton *b = [UIButton buttonWithType:UIButtonTypeCustom];
    [b setTitle:@"Lưu hàng chờ thành playlist" forState:UIControlStateNormal];
    [b setTitleColor:[GMTheme accent] forState:UIControlStateNormal];
    [b setTitleColor:[[GMTheme accent] colorWithAlphaComponent:0.4] forState:UIControlStateHighlighted];
    b.titleLabel.font = [UIFont boldSystemFontOfSize:13];
    b.layer.cornerRadius = 6;
    b.layer.borderWidth = 1;
    b.layer.borderColor = [[GMTheme accent] colorWithAlphaComponent:0.6].CGColor;
    b.backgroundColor = [GMTheme glassFill];
    b.frame = CGRectMake(12, 8, w - 24, 32);
    b.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [b addTarget:self action:@selector(saveQueue) forControlEvents:UIControlEventTouchUpInside];
    [h addSubview:b];
    self.tableView.tableHeaderView = h;
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(changed) name:GMPlayerChangedNotification object:nil];
    [self load];
}

- (void)saveQueue {
    NSArray *keys = [GMPlayer shared].queueKeys;
    if (!keys.count) return;
    UIView *host = self.view;
    GMAskText(@"Lưu hàng chờ", @"Tên playlist", @"Hàng chờ", ^(NSString *name) {
        [GMCore call:@"pl.create" args:@{@"name": name, @"keys": keys} done:^(id ok, NSDictionary *err) {
            [GMTheme toast:err ? GMErrorText(err) : [NSString stringWithFormat:@"Đã lưu «%@» (%lu bài)", name, (unsigned long)keys.count]
                        in:host];
        }];
    });
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

- (void)load {
    GMPlayer *p = [GMPlayer shared];
    _keys = p.queueKeys;
    _index = p.index;
    [self.tableView reloadData];
}

- (void)changed {
    if (_mutating) return;
    GMPlayer *p = [GMPlayer shared];
    // Đang sửa mà bài vẫn vậy: giữ bảng (khỏi phá thao tác kéo); bài đã đổi thì phải tải lại — chỉ số cũ sai
    if (self.tableView.editing && p.index == _index) return;
    if (p.index != _index || p.queueKeys.count != _keys.count) [self load];
}

- (NSInteger)upcoming {
    return MAX(0, (NSInteger)_keys.count - _index - 1);
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return 2;
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    if (s == 0) return @"Đang phát";
    return [self upcoming] ? [NSString stringWithFormat:@"Tiếp theo (%ld bài)", (long)[self upcoming]] : @"Hết hàng chờ";
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    if (_index < 0 || _index >= (NSInteger)_keys.count) return 0;
    return s == 0 ? 1 : [self upcoming];
}

- (NSInteger)queueIndex:(NSIndexPath *)ip {
    return ip.section == 0 ? _index : _index + 1 + ip.row;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    GMSongCell *c = (GMSongCell *)[tv dequeueReusableCellWithIdentifier:@"q"];
    if (!c) c = [[GMSongCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:@"q"];
    NSInteger qi = [self queueIndex:ip];
    NSString *key = [_keys objectAtIndex:qi];
    NSDictionary *s = [[GMPlayer shared] infoFor:key];
    if (!s) {
        [c setLoading];
        // Lấy thông tin một cửa sổ quanh ô này
        NSInteger a = MAX(0, qi - 5), b = MIN((NSInteger)_keys.count, qi + 40);
        [[GMPlayer shared] fetchInfo:[_keys subarrayWithRange:NSMakeRange(a, b - a)] done:^{
            NSArray *vis = [tv indexPathsForVisibleRows];
            if (vis.count) [tv reloadRowsAtIndexPaths:vis withRowAnimation:UITableViewRowAnimationNone];
        }];
        return c;
    }
    [c setSong:s current:ip.section == 0 progress:-1 showSource:YES];
    c.more = nil;
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    if (ip.section == 1) [[GMPlayer shared] jumpTo:[self queueIndex:ip]];
}

- (BOOL)tableView:(UITableView *)tv canEditRowAtIndexPath:(NSIndexPath *)ip {
    return ip.section == 1;
}

- (BOOL)tableView:(UITableView *)tv canMoveRowAtIndexPath:(NSIndexPath *)ip {
    return ip.section == 1;
}

- (NSIndexPath *)tableView:(UITableView *)tv targetIndexPathForMoveFromRowAtIndexPath:(NSIndexPath *)from
       toProposedIndexPath:(NSIndexPath *)to {
    return to.section == 1 ? to : [NSIndexPath indexPathForRow:0 inSection:1];
}

- (void)tableView:(UITableView *)tv moveRowAtIndexPath:(NSIndexPath *)from toIndexPath:(NSIndexPath *)to {
    GMPlayer *p = [GMPlayer shared];
    if (p.index != _index) {
        [self load];
        return;
    }
    _mutating = YES;
    [p moveFrom:[self queueIndex:from] to:[self queueIndex:to]];
    _mutating = NO;
    _keys = p.queueKeys;
    _index = p.index;
}

- (NSString *)tableView:(UITableView *)tv titleForDeleteConfirmationButtonForRowAtIndexPath:(NSIndexPath *)ip {
    return @"Bỏ";
}

- (void)tableView:(UITableView *)tv commitEditingStyle:(UITableViewCellEditingStyle)style forRowAtIndexPath:(NSIndexPath *)ip {
    if (style != UITableViewCellEditingStyleDelete) return;
    GMPlayer *p = [GMPlayer shared];
    if (p.index != _index) {
        [self load];  // bài đã đổi: chỉ số của ô không còn đúng
        return;
    }
    NSInteger before = [self upcoming];
    _mutating = YES;
    [p removeAt:[self queueIndex:ip]];
    _mutating = NO;
    _keys = p.queueKeys;
    _index = p.index;
    if (before - [self upcoming] == 1) [tv deleteRowsAtIndexPaths:@[ip] withRowAnimation:UITableViewRowAnimationAutomatic];
    else [tv reloadData];
}

@end
