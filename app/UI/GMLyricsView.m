#import "GMLyricsView.h"
#import "GMCore.h"
#import "GMPlayer.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

/// Việc của dòng chữ giữa khung khi chưa có lời.
typedef enum { GMLyNone, GMLyLoading, GMLyOff, GMLyFailed } GMLyState;

@interface GMLyricsView () <UITableViewDataSource, UITableViewDelegate, UIAlertViewDelegate>
@end

@implementation GMLyricsView {
    UITableView *_table;
    UILabel *_msg;
    UIActivityIndicatorView *_spin;
    NSString *_key;
    NSDictionary *_song;
    NSArray *_lines;   // lời chạy: [{t, s}]; lời thường: [NSString]
    BOOL _synced;
    NSInteger _cur;
    NSDate *_userScroll;  // lần tự cuộn gần nhất — 4 giây sau mới tự cuộn theo bài tiếp
    NSUInteger _req;
    GMLyState _state;
}

- (id)initWithFrame:(CGRect)f {
    self = [super initWithFrame:f];
    if (self) {
        // Phủ tối lên ảnh bìa: chữ trắng đọc được trên mọi ảnh, cả giao diện sáng lẫn tối
        self.backgroundColor = [UIColor colorWithWhite:0 alpha:0.62];
        self.layer.cornerRadius = 8;
        self.clipsToBounds = YES;
        _table = [[UITableView alloc] initWithFrame:self.bounds style:UITableViewStylePlain];
        _table.tag = GMTagNoWallpaper;
        _table.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
        _table.backgroundColor = [UIColor clearColor];
        _table.backgroundView = nil;
        _table.separatorStyle = UITableViewCellSeparatorStyleNone;
        _table.showsVerticalScrollIndicator = NO;
        _table.dataSource = self;
        _table.delegate = self;
        [self addSubview:_table];
        _msg = [[UILabel alloc] init];
        _msg.backgroundColor = [UIColor clearColor];
        _msg.textColor = [UIColor colorWithWhite:1 alpha:0.85];
        _msg.font = [UIFont systemFontOfSize:15];
        _msg.textAlignment = GMAlignCenter;
        _msg.numberOfLines = 0;
        _msg.userInteractionEnabled = YES;
        [_msg addGestureRecognizer:[[UITapGestureRecognizer alloc] initWithTarget:self action:@selector(tapMessage)]];
        [self addSubview:_msg];
        _spin = [[UIActivityIndicatorView alloc] initWithActivityIndicatorStyle:UIActivityIndicatorViewStyleWhite];
        _spin.hidesWhenStopped = YES;
        [self addSubview:_spin];
        _cur = -1;
    }
    return self;
}

- (void)layoutSubviews {
    [super layoutSubviews];
    CGSize s = self.bounds.size;
    _table.frame = self.bounds;
    // Dòng đầu / cuối cũng cuộn được ra giữa khung
    CGFloat pad = MAX(0, floor(s.height / 2 - 24));
    _table.contentInset = UIEdgeInsetsMake(pad, 0, pad, 0);
    _msg.frame = CGRectMake(16, 0, s.width - 32, s.height);
    _spin.center = CGPointMake(s.width / 2, s.height / 2 - 34);
}

#pragma mark - Tải lời

- (void)showSong:(NSDictionary *)song {
    NSString *key = [GMVal(song, @"loading") boolValue] ? nil : GMVal(song, @"key");
    if (key && [key isEqual:_key]) return;
    _key = key;
    _song = song;
    _lines = nil;
    _cur = -1;
    [_table reloadData];
    if (!key) {
        // Bài đang chờ thông tin: bỏ kết quả của lần tìm lời đang chạy cho bài trước
        _req++;
        [self state:GMLyLoading text:@""];
        return;
    }
    [self load:NO];
}

- (void)reload {
    if (_key) [self load:NO];
}

- (void)state:(GMLyState)st text:(NSString *)text {
    _state = st;
    _msg.text = text;
    _msg.hidden = text == nil;
    _table.hidden = text != nil;
    if (st == GMLyLoading) [_spin startAnimating];
    else [_spin stopAnimating];
}

- (void)load:(BOOL)refresh {
    NSString *key = _key;
    if (!key) return;
    NSUInteger req = ++_req;
    [self state:GMLyLoading text:@"Đang tìm lời…"];
    NSMutableDictionary *args = [NSMutableDictionary dictionaryWithObject:key forKey:@"key"];
    if (refresh) [args setObject:@YES forKey:@"refresh"];
    // Nhạc trên máy (không có trong thư viện của lõi): gửi kèm tên / ca sĩ / thời lượng
    if ([key hasPrefix:@"ipod:"] || [key hasPrefix:@"file:"]) {
        [args setObject:GMVal(_song, @"title") ?: @"" forKey:@"title"];
        [args setObject:GMVal(_song, @"artist") ?: @"" forKey:@"artist"];
        [args setObject:GMVal(_song, @"dur") ?: @0 forKey:@"dur"];
    }
    [GMCore call:@"lyrics.get" args:args done:^(id ok, NSDictionary *err) {
        if (req != _req) return;
        if (err) {
            if ([GMVal(err, @"code") isEqual:@"LYRICS_OFF"]) {
                [self state:GMLyOff text:@"Lời bài hát đang tắt.\n\nChạm để bật."];
            } else {
                [self state:GMLyFailed text:[NSString stringWithFormat:@"Không tải được lời.\n%@\n\nChạm để thử lại.", GMErrorText(err)]];
            }
            return;
        }
        [self apply:ok];
    }];
}

- (void)apply:(NSDictionary *)r {
    if (![GMVal(r, @"found") boolValue]) {
        [self state:GMLyNone text:@"Chưa có lời cho bài này."];
        return;
    }
    _synced = [GMVal(r, @"synced") boolValue];
    NSString *plain = GMVal(r, @"plain") ?: @"";
    if (_synced) {
        _lines = GMVal(r, @"lines");
    } else {
        NSString *p = [plain stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
        _lines = p.length ? [p componentsSeparatedByString:@"\n"] : nil;
    }
    if (!_lines.count) {
        [self state:GMLyNone text:[GMVal(r, @"instrumental") boolValue] ? @"Bài không lời." : @"Chưa có lời cho bài này."];
        return;
    }
    [self state:GMLyNone text:nil];
    _cur = -1;
    _userScroll = nil;
    // Lời thường (LRCLIB không có bản có mốc thời gian): nói rõ vì sao lời không chạy theo nhạc
    if (_synced) {
        _table.tableHeaderView = nil;
    } else {
        CGFloat w = _table.bounds.size.width;
        UILabel *l = [[UILabel alloc] initWithFrame:CGRectMake(12, 0, w - 24, 44)];
        l.text = @"Lời chưa có mốc thời gian — cuộn tay để xem.";
        l.font = [UIFont italicSystemFontOfSize:12];
        l.textColor = [UIColor colorWithWhite:1 alpha:0.55];
        l.backgroundColor = [UIColor clearColor];
        l.textAlignment = GMAlignCenter;
        l.numberOfLines = 2;
        l.autoresizingMask = UIViewAutoresizingFlexibleWidth;
        UIView *h = [[UIView alloc] initWithFrame:CGRectMake(0, 0, w, 48)];
        [h addSubview:l];
        _table.tableHeaderView = h;
    }
    [_table reloadData];
    [_table setContentOffset:CGPointMake(0, -_table.contentInset.top) animated:NO];
    if (_synced) [self setTime:[GMPlayer shared].position];
}

- (void)tapMessage {
    if (_state == GMLyFailed) {
        [self load:YES];
    } else if (_state == GMLyOff) {
        [[[UIAlertView alloc] initWithTitle:@"Bật lời bài hát?"
                                    message:@"Tên bài, ca sĩ và thời lượng của bài đang nghe được gửi tới LRCLIB (lrclib.net — kho lời bài "
                                            @"hát miễn phí) để tìm lời. Không gửi gì khác."
                                   delegate:self cancelButtonTitle:@"Huỷ" otherButtonTitles:@"Bật", nil] show];
    }
}

- (void)alertView:(UIAlertView *)a clickedButtonAtIndex:(NSInteger)i {
    if (i == a.cancelButtonIndex) return;
    [GMCore call:@"settings.set" args:@{@"lyrics": @YES} done:^(id ok, NSDictionary *err) {
        if (!err) [self load:NO];
    }];
}

#pragma mark - Dòng đang hát

- (double)timeAt:(NSInteger)i {
    return [GMVal([_lines objectAtIndex:i], @"t") doubleValue];
}

- (void)setTime:(double)t {
    if (!_synced || !_lines.count || !_table.window) return;
    // Dòng cuối cùng đã tới mốc (sớm 0,3 giây cho kịp đọc)
    NSInteger lo = 0, hi = (NSInteger)_lines.count - 1, idx = -1;
    while (lo <= hi) {
        NSInteger m = (lo + hi) / 2;
        if ([self timeAt:m] <= t + 0.3) {
            idx = m;
            lo = m + 1;
        } else {
            hi = m - 1;
        }
    }
    if (idx == _cur) return;
    NSInteger old = _cur;
    _cur = idx;
    NSMutableArray *ips = [NSMutableArray array];
    if (old >= 0 && old < (NSInteger)_lines.count) [ips addObject:[NSIndexPath indexPathForRow:old inSection:0]];
    if (idx >= 0) [ips addObject:[NSIndexPath indexPathForRow:idx inSection:0]];
    [_table reloadRowsAtIndexPaths:ips withRowAnimation:UITableViewRowAnimationNone];
    BOOL userBusy = _table.dragging || _table.decelerating || (_userScroll && -[_userScroll timeIntervalSinceNow] < 4);
    if (idx >= 0 && !userBusy) {
        // Tự tính cho dòng đang hát nằm giữa khung (scrollToRow…Middle của iOS 5 tính lệch khi bảng có contentInset — iPad 1:
        // dòng đang hát dính mép trên)
        CGRect r = [_table rectForRowAtIndexPath:[NSIndexPath indexPathForRow:idx inSection:0]];
        UIEdgeInsets in = _table.contentInset;
        CGFloat y = CGRectGetMidY(r) - _table.bounds.size.height / 2;
        CGFloat maxY = MAX(-in.top, _table.contentSize.height + in.bottom - _table.bounds.size.height);
        y = MIN(MAX(y, -in.top), maxY);
        [_table setContentOffset:CGPointMake(0, floor(y)) animated:YES];
    }
}

- (void)scrollViewWillBeginDragging:(UIScrollView *)sv {
    _userScroll = [NSDate date];
}

- (void)scrollViewDidEndDecelerating:(UIScrollView *)sv {
    _userScroll = [NSDate date];
}

- (void)scrollViewDidEndDragging:(UIScrollView *)sv willDecelerate:(BOOL)d {
    _userScroll = [NSDate date];
}

#pragma mark - Bảng

- (NSString *)textAt:(NSInteger)i {
    id l = [_lines objectAtIndex:i];
    NSString *s = [l isKindOfClass:[NSString class]] ? l : GMVal(l, @"s");
    s = [s stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    // Lời chạy: dòng trống là đoạn nhạc dạo
    return s.length ? s : (_synced ? @"♪" : @" ");
}

- (UIFont *)fontCurrent:(BOOL)cur {
    return cur ? [UIFont boldSystemFontOfSize:18] : [UIFont systemFontOfSize:_synced ? 16 : 15];
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return _lines.count;
}

- (CGFloat)tableView:(UITableView *)tv heightForRowAtIndexPath:(NSIndexPath *)ip {
    // Đo theo chữ đậm (to nhất) cho mọi dòng → dòng đổi đậm / nhạt không làm bảng nhảy
    CGSize sz = [[self textAt:ip.row] sizeWithFont:[self fontCurrent:_synced] constrainedToSize:CGSizeMake(tv.bounds.size.width - 32, 400)
                                     lineBreakMode:GMWordWrap];
    return ceil(sz.height) + (_synced ? 14 : 6);
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"ly"];
    if (!c) {
        c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:@"ly"];
        c.tag = GMTagOwnBackground;
        c.backgroundColor = [UIColor clearColor];
        c.backgroundView = [[UIView alloc] init];
        c.selectionStyle = UITableViewCellSelectionStyleNone;
        c.textLabel.numberOfLines = 0;
        c.textLabel.textAlignment = GMAlignCenter;
        c.textLabel.backgroundColor = [UIColor clearColor];
    }
    BOOL cur = _synced && ip.row == _cur;
    c.textLabel.text = [self textAt:ip.row];
    c.textLabel.font = [self fontCurrent:cur];
    c.textLabel.textColor = cur ? [UIColor whiteColor] : [UIColor colorWithWhite:1 alpha:_synced ? 0.5 : 0.85];
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    // Chạm một dòng (lời chạy): tua tới dòng đó
    if (!_synced) return;
    _userScroll = nil;
    [[GMPlayer shared] seek:[self timeAt:ip.row]];
    [self setTime:[self timeAt:ip.row]];
}

@end
