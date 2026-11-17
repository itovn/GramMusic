#import "GMSourcesViewController.h"
#import "GMArt.h"
#import "GMCore.h"
#import "GMImages.h"
#import "GMLocalViewController.h"
#import "GMSongActions.h"
#import "GMSongsViewController.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

@interface GMSourcesViewController () <UIAlertViewDelegate>
@end

@implementation GMSourcesViewController {
    NSArray *_sources;
    NSDictionary *_sync;
    UILabel *_status;
    UIButton *_fresh, *_full;
    NSTimer *_reloadTimer;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Nguồn";
    self.tableView.rowHeight = 52;
    self.navigationItem.rightBarButtonItem =
        [[UIBarButtonItem alloc] initWithTitle:@"Thêm" style:UIBarButtonItemStyleBordered target:self action:@selector(addSource)];
    [self buildHeader];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(event:) name:GMCoreEventNotification object:nil];
    [self load];
    [GMCore call:@"sync.state" args:nil done:^(id ok, NSDictionary *err) {
        if ([ok isKindOfClass:[NSDictionary class]]) {
            _sync = ok;
            [self updateStatus];
        }
    }];
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
    [_reloadTimer invalidate];
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    [self load];
}

- (UIButton *)button:(NSString *)title action:(SEL)sel {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeCustom];
    [b setTitle:title forState:UIControlStateNormal];
    [b setTitleColor:[GMTheme accent] forState:UIControlStateNormal];
    [b setTitleColor:[[GMTheme accent] colorWithAlphaComponent:0.4] forState:UIControlStateHighlighted];
    [b setTitleColor:[GMTheme secondaryText] forState:UIControlStateDisabled];
    b.titleLabel.font = [UIFont boldSystemFontOfSize:13];
    b.layer.cornerRadius = 6;
    b.layer.borderWidth = 1;
    b.layer.borderColor = [[GMTheme accent] colorWithAlphaComponent:0.6].CGColor;
    b.backgroundColor = [GMTheme glassFill];
    [b addTarget:self action:sel forControlEvents:UIControlEventTouchUpInside];
    return b;
}

- (void)buildHeader {
    CGFloat w = self.tableView.bounds.size.width;
    UIView *h = [[UIView alloc] initWithFrame:CGRectMake(0, 0, w, 84)];
    _status = [[UILabel alloc] initWithFrame:CGRectMake(12, 8, w - 24, 32)];
    _status.numberOfLines = 2;
    _status.font = [UIFont systemFontOfSize:12];
    _status.textColor = [GMTheme secondaryText];
    _status.backgroundColor = [UIColor clearColor];
    _status.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [h addSubview:_status];
    _fresh = [self button:@"Lấy bài mới" action:@selector(syncNew)];
    _fresh.frame = CGRectMake(12, 44, 120, 30);
    [h addSubview:_fresh];
    _full = [self button:@"Quét toàn bộ" action:@selector(syncFull)];
    _full.frame = CGRectMake(140, 44, 120, 30);
    [h addSubview:_full];
    self.tableView.tableHeaderView = h;
    [self updateStatus];
}

- (void)updateStatus {
    BOOL running = [GMVal(_sync, @"running") boolValue];
    NSInteger found = [GMVal(_sync, @"found") integerValue];
    NSInteger wait = [GMVal(_sync, @"wait") integerValue];
    NSString *phase = GMVal(_sync, @"phase");
    if (running && wait > 0) {
        _status.text = [NSString stringWithFormat:@"Telegram bảo chờ %ld giây (lần %@/3) rồi quét tiếp…", (long)wait, GMVal(_sync, @"try") ?: @1];
    } else if (running) {
        NSInteger ci = [GMVal(_sync, @"chat") integerValue], cn = [GMVal(_sync, @"chats") integerValue];
        NSString *what = [phase isEqual:@"document"] ? @"tệp nhạc gửi dạng tài liệu" : @"bài hát";
        _status.text = cn > 0 ? [NSString stringWithFormat:@"Đang quét chat %ld/%ld (%@)… %ld bài mới.", (long)ci, (long)cn, what, (long)found]
                              : [NSString stringWithFormat:@"Đang quét %@… %ld bài mới.", what, (long)found];
    } else if (GMVal(_sync, @"error")) {
        _status.text = [NSString stringWithFormat:@"Quét dừng: %@", GMErrorText(GMVal(_sync, @"error"))];
    } else {
        NSInteger n = _sources.count;
        _status.text = [NSString stringWithFormat:@"%ld nguồn nhạc. Nhạc trong Tin nhắn đã lưu, nhóm, kênh của bạn tự hiện ở đây.", (long)n];
    }
    _fresh.enabled = _full.enabled = !running;
    [_full setTitle:running ? @"Dừng quét" : @"Quét toàn bộ" forState:UIControlStateNormal];
    _full.enabled = YES;
}

- (void)load {
    [GMCore call:@"sources.list" args:nil done:^(id ok, NSDictionary *err) {
        if (![ok isKindOfClass:[NSArray class]]) return;
        _sources = ok;
        [self.tableView reloadData];
        [self updateStatus];
    }];
}

- (void)event:(NSNotification *)n {
    NSString *e = GMVal(n.userInfo, @"e");
    if ([e isEqual:@"sync.state"]) {
        _sync = n.userInfo;
        [self updateStatus];
        if (![GMVal(_sync, @"running") boolValue]) [self load];
    } else if ([e isEqual:@"library.changed"]) {
        [_reloadTimer invalidate];
        _reloadTimer = [NSTimer scheduledTimerWithTimeInterval:2 target:self selector:@selector(load) userInfo:nil repeats:NO];
    }
}

- (void)syncNew {
    [GMCore call:@"sync.start" args:@{} done:nil];
}

- (void)syncFull {
    if ([GMVal(_sync, @"running") boolValue]) {
        [GMCore call:@"sync.stop" args:nil done:nil];
        return;
    }
    [[[UIAlertView alloc] initWithTitle:@"Quét toàn bộ tài khoản?"
                                message:@"Đọc lại mọi bài hát trong các chat, nhóm, kênh của bạn. Tài khoản nhiều nhạc có thể mất vài phút; "
                                        @"tắt app giữa chừng thì lần sau quét tiếp."
                               delegate:self cancelButtonTitle:@"Huỷ" otherButtonTitles:@"Quét", nil] show];
}

- (void)alertView:(UIAlertView *)a clickedButtonAtIndex:(NSInteger)i {
    if (i == 1) [GMCore call:@"sync.start" args:@{@"full": @YES} done:nil];
}

- (void)addSource {
    GMAskText(@"Thêm kênh / nhóm công khai", @"@tênkênh hoặc https://t.me/…", nil, ^(NSString *link) {
        [GMTheme toast:@"Đang tìm kênh…" in:self.view];
        [GMCore call:@"sources.add" args:@{@"link": link} done:^(id ok, NSDictionary *err) {
            if (err) {
                [[[UIAlertView alloc] initWithTitle:@"Không thêm được" message:GMErrorText(err) delegate:nil cancelButtonTitle:@"OK"
                                  otherButtonTitles:nil] show];
                return;
            }
            [GMTheme toast:@"Đã thêm — đang quét bài của kênh" in:self.view];
            [self load];
        }];
    });
}

#pragma mark - Bảng

/// Nhóm 0: nhạc trên máy (app Nhạc, tệp); nhóm 1: nguồn Telegram.
- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return 2;
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    return s == 0 ? @"Trên máy" : @"Telegram";
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return s == 0 ? 2 : _sources.count;
}

static NSString *GMKindText(NSString *k) {
    if ([k isEqual:@"self"]) return @"Tin nhắn đã lưu";
    if ([k isEqual:@"channel"]) return @"Kênh";
    if ([k isEqual:@"group"]) return @"Nhóm";
    if ([k isEqual:@"bot"]) return @"Bot";
    return @"Chat riêng";
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    if (ip.section == 0) {
        UITableViewCell *l = [tv dequeueReusableCellWithIdentifier:@"loc"];
        if (!l) {
            l = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:@"loc"];
            l.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
        }
        l.textLabel.font = [UIFont boldSystemFontOfSize:15];
        l.textLabel.text = ip.row == 0 ? @"Nhạc trên máy" : @"Tệp trên máy";
        l.detailTextLabel.text = ip.row == 0 ? @"Bài chép từ iTunes (app Nhạc)" : @"mp3, m4a, FLAC, OGG, Opus… trong máy";
        l.imageView.image = [GMTheme settingsIcon:ip.row == 0 ? @"music" : @"folders"];
        return l;
    }
    UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"src"];
    if (!c) {
        c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:@"src"];
        UISwitch *sw = [[UISwitch alloc] init];
        [sw addTarget:self action:@selector(switched:) forControlEvents:UIControlEventValueChanged];
        c.accessoryView = sw;
    }
    NSDictionary *s = [_sources objectAtIndex:ip.row];
    NSString *peer = GMVal(s, @"peer");
    NSString *kind = GMVal(s, @"kind");
    c.textLabel.text = GMVal(s, @"name");
    c.textLabel.font = [UIFont boldSystemFontOfSize:15];
    NSInteger n = [GMVal(s, @"count") integerValue];
    NSString *extra = [GMVal(s, @"forum") boolValue] ? @" · có chủ đề ›" : ([GMVal(s, @"manual") boolValue] ? @" · thêm tay" : @"");
    c.detailTextLabel.text = [NSString stringWithFormat:@"%@ · %ld bài%@", GMKindText(kind), (long)n, extra];
    UISwitch *sw = (UISwitch *)c.accessoryView;
    sw.on = [GMVal(s, @"enabled") boolValue];
    sw.tag = ip.row;
    c.imageView.image = [kind isEqual:@"self"] ? [GMTheme settingsIcon:@"saved"] : [GMTheme avatarFor:peer name:GMVal(s, @"name") size:32];
    c.imageView.layer.cornerRadius = 0;
    NSString *photo = GMVal(s, @"photo");
    if (photo && ![kind isEqual:@"self"]) {
        __weak UITableViewCell *wc = c;
        [GMImages avatarForPeer:peer photo:photo dc:GMVal(s, @"photo_dc") size:32 done:^(UIImage *img) {
            NSIndexPath *now = [tv indexPathForCell:wc];
            if (now && now.section == 1 && now.row < (NSInteger)_sources.count &&
                [GMVal([_sources objectAtIndex:now.row], @"peer") isEqual:peer]) {
                wc.imageView.image = img;
            }
        }];
    }
    return c;
}

- (void)switched:(UISwitch *)sw {
    if (sw.tag >= (NSInteger)_sources.count) return;
    NSDictionary *s = [_sources objectAtIndex:sw.tag];
    [GMCore call:@"sources.set" args:@{@"peer": GMVal(s, @"peer"), @"enabled": @(sw.on)} done:^(id ok, NSDictionary *err) {
        if (err) [GMTheme toast:GMErrorText(err) in:self.view];
        else [GMTheme toast:sw.on ? @"Hiện trong Thư viện" : @"Đã ẩn khỏi Thư viện" in:self.view];
        [self load];
    }];
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    if (ip.section == 0) {
        UIViewController *v = ip.row == 0 ? [[GMIPodSongsViewController alloc] initWithStyle:UITableViewStylePlain]
                                          : [[GMFilesViewController alloc] initWithPath:nil title:nil];
        [self.navigationController pushViewController:v animated:YES];
        return;
    }
    NSDictionary *s = [_sources objectAtIndex:ip.row];
    if ([GMVal(s, @"forum") boolValue]) {
        GMTopicsViewController *t = [[GMTopicsViewController alloc] initWithStyle:UITableViewStylePlain];
        t.peer = GMVal(s, @"peer");
        t.title = GMVal(s, @"name");
        [self.navigationController pushViewController:t animated:YES];
        return;
    }
    GMSongsViewController *v = [[GMSongsViewController alloc] initWithKind:GMSongsSource];
    v.source = GMVal(s, @"peer");
    v.title = GMVal(s, @"name");
    [self.navigationController pushViewController:v animated:YES];
}

- (BOOL)tableView:(UITableView *)tv canEditRowAtIndexPath:(NSIndexPath *)ip {
    return ip.section == 1;
}

- (NSString *)tableView:(UITableView *)tv titleForDeleteConfirmationButtonForRowAtIndexPath:(NSIndexPath *)ip {
    return @"Gỡ";
}

- (void)tableView:(UITableView *)tv commitEditingStyle:(UITableViewCellEditingStyle)style forRowAtIndexPath:(NSIndexPath *)ip {
    if (style != UITableViewCellEditingStyleDelete) return;
    NSDictionary *s = [_sources objectAtIndex:ip.row];
    // Nguồn từ quét tài khoản sẽ quay lại ở lần quét sau (chat vẫn còn nhạc) — muốn bỏ hẳn thì tắt công tắc
    [GMCore call:@"sources.remove" args:@{@"peer": GMVal(s, @"peer")} done:^(id ok, NSDictionary *err) {
        if (err) [GMTheme toast:GMErrorText(err) in:self.view];
        else if (![GMVal(s, @"manual") boolValue]) [GMTheme toast:@"Đã gỡ — lần quét sau sẽ hiện lại; muốn ẩn hẳn thì tắt công tắc" in:self.view];
        [self load];
    }];
}

@end

#pragma mark - Chủ đề của nhóm

@implementation GMTopicsViewController {
    NSArray *_topics;
    NSInteger _total;
    NSString *_name;
    UIButton *_saveAll;
    UILabel *_hint;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.tableView.rowHeight = 50;
    CGFloat w = self.tableView.bounds.size.width;
    UIView *h = [[UIView alloc] initWithFrame:CGRectMake(0, 0, w, 72)];
    _saveAll = [UIButton buttonWithType:UIButtonTypeCustom];
    [_saveAll setTitle:@"Mỗi chủ đề thành một playlist" forState:UIControlStateNormal];
    [_saveAll setTitleColor:[GMTheme accent] forState:UIControlStateNormal];
    [_saveAll setTitleColor:[[GMTheme accent] colorWithAlphaComponent:0.4] forState:UIControlStateHighlighted];
    _saveAll.titleLabel.font = [UIFont boldSystemFontOfSize:13];
    _saveAll.layer.cornerRadius = 6;
    _saveAll.layer.borderWidth = 1;
    _saveAll.layer.borderColor = [[GMTheme accent] colorWithAlphaComponent:0.6].CGColor;
    _saveAll.backgroundColor = [GMTheme glassFill];
    _saveAll.frame = CGRectMake(12, 8, w - 24, 32);
    _saveAll.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [_saveAll addTarget:self action:@selector(saveAll) forControlEvents:UIControlEventTouchUpInside];
    [h addSubview:_saveAll];
    _hint = [[UILabel alloc] initWithFrame:CGRectMake(12, 44, w - 24, 24)];
    _hint.font = [UIFont systemFontOfSize:12];
    _hint.textColor = [GMTheme secondaryText];
    _hint.backgroundColor = [UIColor clearColor];
    _hint.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    _hint.text = @"Đang lấy danh sách chủ đề…";
    [h addSubview:_hint];
    self.tableView.tableHeaderView = h;
    [self load];
}

- (void)load {
    [GMCore call:@"sources.topics" args:@{@"peer": _peer ?: @""} done:^(id ok, NSDictionary *err) {
        if (err) {
            _hint.text = GMErrorText(err);
            return;
        }
        _topics = GMVal(ok, @"topics") ?: @[];
        _total = [GMVal(ok, @"total") integerValue];
        _name = GMVal(ok, @"name");
        _hint.text = [NSString stringWithFormat:@"%lu chủ đề có nhạc · %ld bài", (unsigned long)_topics.count, (long)_total];
        _saveAll.enabled = _topics.count > 0;
        [self.tableView reloadData];
    }];
}

/// Tạo (hoặc bổ sung) playlist «Nhóm · Chủ đề» cho từng chủ đề, lần lượt.
- (void)saveAll {
    NSArray *topics = _topics;
    if (!topics.count) return;
    _saveAll.enabled = NO;
    __block NSInteger i = 0, made = 0;
    __block void (^step)(void);
    __weak GMTopicsViewController *ws = self;
    UIView *host = self.view;
    NSString *group = _name ?: self.title ?: @"Nhóm";
    step = [^{
        if (i >= (NSInteger)topics.count) {
            [GMTheme toast:[NSString stringWithFormat:@"Đã lưu %ld playlist (mục Playlist)", (long)made] in:host];
            GMTopicsViewController *me = ws;
            if (me) me->_saveAll.enabled = YES;
            step = nil;
            return;
        }
        NSDictionary *t = [topics objectAtIndex:i++];
        NSString *name = [NSString stringWithFormat:@"%@ · %@", group, GMVal(t, @"title")];
        [GMCore call:@"pl.fromQuery" args:@{@"name": name, @"source": ws.peer ?: @"", @"topic": GMVal(t, @"id") ?: @1, @"sort": @"date"}
                done:^(id ok, NSDictionary *err) {
                    if (!err) made++;
                    if (step) step();
                }];
    } copy];
    step();
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return 2;
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    return s == 1 && _topics.count ? @"Chủ đề" : nil;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return s == 0 ? (_topics ? 1 : 0) : _topics.count;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"t"] ?:
        [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:@"t"];
    c.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
    c.textLabel.font = [UIFont boldSystemFontOfSize:15];
    if (ip.section == 0) {
        c.textLabel.text = @"Tất cả bài của nhóm";
        c.detailTextLabel.text = [NSString stringWithFormat:@"%ld bài", (long)_total];
        c.imageView.image = [GMArt glyph:@"queue" size:30 color:[GMTheme accent]];
        return c;
    }
    NSDictionary *t = [_topics objectAtIndex:ip.row];
    c.textLabel.text = GMVal(t, @"title");
    c.detailTextLabel.text = [NSString stringWithFormat:@"%@ bài", GMVal(t, @"count")];
    c.imageView.image = [GMTheme topicIcon:GMVal(t, @"title") color:[GMVal(t, @"color") integerValue] size:30];
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    GMSongsViewController *v = [[GMSongsViewController alloc] initWithKind:GMSongsSource];
    v.source = _peer;
    v.forum = YES;
    if (ip.section == 0) {
        v.title = self.title;
    } else {
        NSDictionary *t = [_topics objectAtIndex:ip.row];
        v.topic = GMVal(t, @"id");
        v.title = GMVal(t, @"title");
    }
    [self.navigationController pushViewController:v animated:YES];
}

@end
