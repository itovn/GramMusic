#import "GMSongsViewController.h"
#import "GMAppDelegate.h"
#import "GMSourcesViewController.h"
#import "GMArt.h"
#import "GMCompat.h"
#import "GMCore.h"
#import "GMLog.h"
#import "GMPlayer.h"
#import "GMRootController.h"
#import "GMSongActions.h"
#import "GMSongCell.h"
#import "GMSongList.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

/// «1.234» (dấu chấm ngăn nghìn).
static NSString *GMCount(NSInteger n) {
    if (n < 1000) return [NSString stringWithFormat:@"%ld", (long)n];
    return [NSString stringWithFormat:@"%@.%03ld", GMCount(n / 1000), (long)(n % 1000)];
}

@implementation GMSongsViewController {
    GMSongList *_list;
    NSArray *_items;       // playlist: cả danh sách
    NSArray *_shown;       // playlist sau khi lọc tìm
    UISearchBar *_search;
    UISegmentedControl *_sortCtl;
    UILabel *_status;
    UIButton *_playAll, *_shuffleAll;
    NSString *_curKey;
    NSMutableDictionary *_progress;
    NSTimer *_searchTimer, *_reloadTimer;
    NSDictionary *_sync;
    NSDictionary *_stats;
    id _refresh;  // UIRefreshControl (iOS 6)
    NSMutableArray *_menu;      // khối của các nút trong bảng thao tác đang mở
    UIBarButtonItem *_savedLeft, *_savedRight;
    BOOL _selecting;
}

- (instancetype)initWithKind:(GMSongsKind)kind {
    if ((self = [super initWithStyle:UITableViewStylePlain])) {
        _kind = kind;
        _progress = [NSMutableDictionary dictionary];
        if (kind != GMSongsPlaylist) {
            _list = [[GMSongList alloc] init];
            NSString *saved = [[NSUserDefaults standardUserDefaults] stringForKey:@"songs.sort"];
            _list.sort = kind == GMSongsOffline ? @"title" : (saved ?: @"date");
            _list.filter = kind == GMSongsOffline ? @"offline" : @"all";
        }
        if (kind == GMSongsLibrary) self.title = @"Thư viện";
        else if (kind == GMSongsOffline) self.title = @"Đã tải";
    }
    return self;
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
    [_searchTimer invalidate];
    [_reloadTimer invalidate];
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.tableView.rowHeight = [GMSongCell height];
    if (_kind == GMSongsSource) {
        _list.source = _source;
        _list.topic = _topic;
    }
    __weak GMSongsViewController *ws = self;
    _list.changed = ^{ [ws listChanged]; };
    [self buildHeader];
    self.navigationItem.rightBarButtonItem =
        [GMTheme barButtonImage:[GMArt glyph:@"more" size:20 color:[GMTheme barText]] target:self action:@selector(showMenu)];
    if (_kind == GMSongsLibrary || _kind == GMSongsSource) {
        Class rc = NSClassFromString(@"UIRefreshControl");
        if (rc) {
            GM_IOS6_BEGIN
            UIRefreshControl *r = [[rc alloc] init];
            [r addTarget:self action:@selector(pulled) forControlEvents:UIControlEventValueChanged];
            self.refreshControl = r;
            _refresh = r;
            GM_IOS6_END
        }
    }
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(coreEvent:) name:GMCoreEventNotification object:nil];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(playerChanged) name:GMPlayerChangedNotification object:nil];
    if (_list) [_list reload];
    if (_kind == GMSongsLibrary) [self loadStats];
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    if (_kind == GMSongsPlaylist) [self loadItems];
    else if (_kind == GMSongsOffline) [_list reload];  // vừa tải / xoá ở màn khác
}

- (void)loggedIn {
    [_list reload];
    [self loadStats];
}

#pragma mark - Đầu bảng

- (UIButton *)smallButton:(NSString *)title glyph:(NSString *)glyph {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeCustom];
    [b setTitle:title forState:UIControlStateNormal];
    [b setTitleColor:[GMTheme accent] forState:UIControlStateNormal];
    [b setTitleColor:[[GMTheme accent] colorWithAlphaComponent:0.4] forState:UIControlStateHighlighted];
    [b setImage:[GMArt glyph:glyph size:14 color:[GMTheme accent]] forState:UIControlStateNormal];
    b.titleLabel.font = [UIFont boldSystemFontOfSize:13];
    b.imageEdgeInsets = UIEdgeInsetsMake(0, -4, 0, 4);
    b.layer.cornerRadius = 6;
    b.layer.borderWidth = 1;
    b.layer.borderColor = [[GMTheme accent] colorWithAlphaComponent:0.6].CGColor;
    b.backgroundColor = [GMTheme glassFill];
    return b;
}

- (void)buildHeader {
    CGFloat w = self.tableView.bounds.size.width;
    UIView *h = [[UIView alloc] initWithFrame:CGRectMake(0, 0, w, 108)];
    h.backgroundColor = [UIColor clearColor];
    _search = [[UISearchBar alloc] initWithFrame:CGRectMake(0, 0, w, 44)];
    _search.placeholder = _kind == GMSongsPlaylist ? @"Tìm trong playlist" : @"Tìm bài hát, ca sĩ (không cần dấu)";
    _search.delegate = self;
    _search.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    // UISearchBar của iOS 5 / 6 không có setKeyboardAppearance: (SDK 9.3 khai báo nên dựng được, chạy thì crash — TubeMini) →
    // đặt cho ô nhập bên trong
    for (UIView *v in _search.subviews) {
        if ([v isKindOfClass:[UITextField class]]) ((UITextField *)v).keyboardAppearance = [GMTheme keyboard];
    }
    [h addSubview:_search];
    _playAll = [self smallButton:@"Phát hết" glyph:@"play"];
    _playAll.frame = CGRectMake(8, 50, 86, 30);
    [_playAll addTarget:self action:@selector(playAll) forControlEvents:UIControlEventTouchUpInside];
    [h addSubview:_playAll];
    _shuffleAll = [self smallButton:@"Ngẫu nhiên" glyph:@"shuffle"];
    _shuffleAll.frame = CGRectMake(100, 50, 98, 30);
    [_shuffleAll addTarget:self action:@selector(shuffleAll) forControlEvents:UIControlEventTouchUpInside];
    [h addSubview:_shuffleAll];
    if (_list) {
        _sortCtl = [[UISegmentedControl alloc] initWithItems:@[@"Mới", @"A–Z", @"Ca sĩ"]];
        _sortCtl.segmentedControlStyle = UISegmentedControlStyleBar;
        _sortCtl.tintColor = [GMTheme accent];
        _sortCtl.frame = CGRectMake(w - 8 - 112, 50, 112, 30);
        _sortCtl.autoresizingMask = UIViewAutoresizingFlexibleLeftMargin;
        _sortCtl.selectedSegmentIndex = [_list.sort isEqual:@"title"] ? 1 : [_list.sort isEqual:@"artist"] ? 2 : 0;
        [_sortCtl addTarget:self action:@selector(sortChanged) forControlEvents:UIControlEventValueChanged];
        [h addSubview:_sortCtl];
    }
    _status = [[UILabel alloc] initWithFrame:CGRectMake(10, 86, w - 20, 18)];
    _status.font = [UIFont systemFontOfSize:12];
    _status.textColor = [GMTheme secondaryText];
    _status.backgroundColor = [UIColor clearColor];
    _status.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [h addSubview:_status];
    self.tableView.tableHeaderView = h;
    [self updateStatus];
}

- (void)updateStatus {
    NSInteger n = _list ? _list.total : (NSInteger)_shown.count;
    NSString *count = [NSString stringWithFormat:@"%@ bài", GMCount(n)];
    BOOL running = [GMVal(_sync, @"running") boolValue];
    if (_kind == GMSongsLibrary && running) {
        NSInteger wait = [GMVal(_sync, @"wait") integerValue];
        NSInteger found = [GMVal(_sync, @"found") integerValue];
        if (wait > 0) _status.text = [NSString stringWithFormat:@"%@ · Telegram bảo chờ %ld giây rồi quét tiếp…", count, (long)wait];
        else {
            NSInteger ci = [GMVal(_sync, @"chat") integerValue], cn = [GMVal(_sync, @"chats") integerValue];
            NSString *where = cn > 0 ? [NSString stringWithFormat:@"chat %ld/%ld, ", (long)ci, (long)cn] : @"";
            _status.text = [NSString stringWithFormat:@"%@ · đang quét Telegram (%@%@ bài mới)…", count, where, GMCount(found)];
        }
    } else if (_kind == GMSongsLibrary && ![GMRoot().me count] && !_list.total) {
        _status.text = @"Chưa đăng nhập Telegram.";
    } else {
        _status.text = count;
    }
    _playAll.enabled = _shuffleAll.enabled = n > 0;
    [self updateEmpty];
}

/// Bảng trống: câu hướng dẫn ở chân bảng.
- (void)updateEmpty {
    BOOL empty = _list ? (_list.loaded && _list.total == 0) : (_items && !_shown.count);
    if (!empty) {
        if (self.tableView.tableFooterView.tag == 77) self.tableView.tableFooterView = [[UIView alloc] initWithFrame:CGRectZero];
        return;
    }
    NSString *text;
    BOOL searching = _search.text.length > 0;
    if (searching) text = @"Không có bài nào khớp.";
    else if (_kind == GMSongsOffline) text = @"Chưa tải bài nào về máy.\nChạm «⋯» cạnh một bài → «Tải về máy» để nghe khi không có mạng.";
    else if (_kind == GMSongsPlaylist) text = @"Playlist trống.\nChạm «⋯» cạnh một bài → «Thêm vào playlist…».";
    else if (_kind == GMSongsSource) text = @"Chưa có bài nào từ nguồn này.\nKéo xuống / bấm làm mới để quét lại.";
    else if ([GMVal(_sync, @"running") boolValue]) text = @"Đang quét nhạc trong tài khoản Telegram…\nBài sẽ hiện dần ở đây.";
    else text = @"Chưa có bài nào.\nNhạc gửi trong Tin nhắn đã lưu, nhóm, kênh của bạn sẽ hiện ở đây sau khi quét. Thêm kênh nhạc "
                @"công khai ở mục «Nguồn».";
    UILabel *l = [[UILabel alloc] initWithFrame:CGRectMake(20, 0, self.tableView.bounds.size.width - 40, 120)];
    l.text = text;
    l.numberOfLines = 0;
    l.textAlignment = GMAlignCenter;
    l.font = [UIFont systemFontOfSize:14];
    l.textColor = [GMTheme secondaryText];
    l.backgroundColor = [UIColor clearColor];
    l.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    UIView *f = [[UIView alloc] initWithFrame:CGRectMake(0, 0, self.tableView.bounds.size.width, 140)];
    f.tag = 77;
    [f addSubview:l];
    l.frame = CGRectMake(20, 20, f.bounds.size.width - 40, 110);
    self.tableView.tableFooterView = f;
}

#pragma mark - Dữ liệu

- (void)loadStats {
    [GMCore call:@"library.stats" args:nil done:^(id ok, NSDictionary *err) {
        if (err) return;
        _stats = ok;
        NSDictionary *sync = GMVal(ok, @"sync");
        if ([sync isKindOfClass:[NSDictionary class]]) _sync = sync;
        [self updateStatus];
    }];
}

- (void)loadItems {
    if (!_playlistId) return;
    [GMCore call:@"pl.items" args:@{@"id": _playlistId} done:^(id ok, NSDictionary *err) {
        if (err) {
            [GMTheme toast:GMErrorText(err) in:self.view];
            return;
        }
        _items = [ok isKindOfClass:[NSArray class]] ? ok : @[];
        [self filterItems];
    }];
}

- (void)filterItems {
    NSString *q = [_search.text stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    if (!q.length) {
        _shown = _items;
    } else {
        NSMutableArray *v = [NSMutableArray array];
        NSStringCompareOptions o = NSCaseInsensitiveSearch | NSDiacriticInsensitiveSearch;
        for (NSDictionary *s in _items) {
            NSString *hay = [NSString stringWithFormat:@"%@ %@", GMVal(s, @"title") ?: @"", GMVal(s, @"artist") ?: @""];
            if ([hay rangeOfString:q options:o].location != NSNotFound) [v addObject:s];
        }
        _shown = v;
    }
    [self.tableView reloadData];
    [self updateStatus];
}

- (void)listChanged {
    if (_selecting) return;
    [self.tableView reloadData];
    [self updateStatus];
    GM_IOS6_BEGIN
    if (_refresh) [(UIRefreshControl *)_refresh endRefreshing];
    GM_IOS6_END
}

/// Có mục chữ cái — theo dữ liệu đã tải (lõi trả `sections` rỗng khi xếp theo ngày); không theo `_list.sort` vì vừa đổi cách xếp
/// thì bảng vẫn giữ bố cục cũ tới khi trang mới về (reloadRows lúc đó → «invalid number of sections»).
- (BOOL)alpha {
    return _list && _list.sections.count > 1;
}

- (NSInteger)globalIndex:(NSIndexPath *)ip {
    if ([self alpha]) return [GMVal([_list.sections objectAtIndex:ip.section], @"i") integerValue] + ip.row;
    return ip.row;
}

- (NSDictionary *)songAt:(NSInteger)g {
    if (_list) return [_list songAt:g];
    return g >= 0 && g < (NSInteger)_shown.count ? [_shown objectAtIndex:g] : nil;
}

- (NSIndexPath *)pathFor:(NSInteger)g {
    if ([self alpha]) {
        for (NSUInteger s = 0; s < _list.sections.count; s++) {
            NSDictionary *sec = [_list.sections objectAtIndex:s];
            NSInteger i = [GMVal(sec, @"i") integerValue], n = [GMVal(sec, @"n") integerValue];
            if (g >= i && g < i + n) return [NSIndexPath indexPathForRow:g - i inSection:s];
        }
        return nil;
    }
    return [NSIndexPath indexPathForRow:g inSection:0];
}

#pragma mark - Bảng

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return [self alpha] ? _list.sections.count : 1;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    if ([self alpha]) return [GMVal([_list.sections objectAtIndex:s], @"n") integerValue];
    return _list ? _list.total : _shown.count;
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    return [self alpha] ? GMVal([_list.sections objectAtIndex:s], @"t") : nil;
}

- (NSArray *)sectionIndexTitlesForTableView:(UITableView *)tv {
    if (![self alpha] || _list.total < 30) return nil;
    NSMutableArray *t = [NSMutableArray array];
    for (NSDictionary *s in _list.sections) [t addObject:GMVal(s, @"t") ?: @"#"];
    return t;
}

- (NSInteger)tableView:(UITableView *)tv sectionForSectionIndexTitle:(NSString *)title atIndex:(NSInteger)i {
    return i;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    GMSongCell *c = (GMSongCell *)[tv dequeueReusableCellWithIdentifier:@"song"];
    if (!c) c = [[GMSongCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:@"song"];
    NSInteger g = [self globalIndex:ip];
    NSDictionary *s = [self songAt:g];
    if (!s) {
        [c setLoading];
        return c;
    }
    NSString *key = GMVal(s, @"key");
    NSNumber *p = [_progress objectForKey:key];
    [c setSong:s current:[key isEqual:GMVal([GMPlayer shared].current, @"key")] progress:p ? p.doubleValue : -1
        showSource:_kind != GMSongsSource];
    __weak GMSongsViewController *ws = self;
    __weak GMSongCell *wc = c;
    c.more = ^{ [ws actionsFor:g anchor:wc]; };
    return c;
}

- (void)actionsFor:(NSInteger)g anchor:(UIView *)anchor {
    NSDictionary *s = [self songAt:g];
    if (!s) return;
    id pl = _kind == GMSongsPlaylist ? _playlistId : (_kind == GMSongsOffline ? @"offline" : nil);
    __weak GMSongsViewController *ws = self;
    [GMSongActions showFor:s in:self from:anchor playlist:pl
        changed:^(NSDictionary *ns) { [ws songChanged:ns]; }
        removed:^{ [ws songRemoved:GMVal(s, @"key")]; }];
}

- (void)songChanged:(NSDictionary *)s {
    if (_list) [_list replaceSong:s];
    else {
        NSMutableArray *v = [_items mutableCopy];
        for (NSUInteger i = 0; i < v.count; i++) {
            if ([GMVal([v objectAtIndex:i], @"key") isEqual:GMVal(s, @"key")]) [v replaceObjectAtIndex:i withObject:s];
        }
        _items = v;
        [self filterItems];
        return;
    }
    [self reloadVisible];
}

- (void)songRemoved:(NSString *)key {
    if (_list) {
        [_list reload];
        return;
    }
    if ([_playlistId isEqual:@"recent"]) {
        [self loadItems];  // «Xoá lịch sử nghe» xoá cả danh sách
        return;
    }
    NSMutableArray *v = [_items mutableCopy];
    for (NSInteger i = (NSInteger)v.count - 1; i >= 0; i--) {
        if ([GMVal([v objectAtIndex:i], @"key") isEqual:key]) [v removeObjectAtIndex:i];
    }
    _items = v;
    [self filterItems];
}

- (void)reloadVisible {
    if (_selecting) return;
    NSArray *vis = [self.tableView indexPathsForVisibleRows];
    if (vis.count) [self.tableView reloadRowsAtIndexPaths:vis withRowAnimation:UITableViewRowAnimationNone];
}

- (void)tableView:(UITableView *)tv didDeselectRowAtIndexPath:(NSIndexPath *)ip {
    if (_selecting) [self updateSelectButtons];
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    if (_selecting) {
        [self updateSelectButtons];
        return;
    }
    [tv deselectRowAtIndexPath:ip animated:YES];
    [_search resignFirstResponder];
    NSInteger g = [self globalIndex:ip];
    NSDictionary *s = [self songAt:g];
    if (!s) return;
    if ([GMVal(s, @"missing") boolValue]) {
        [GMTheme toast:@"Bài không còn trong thư viện (nguồn đã gỡ / tin đã xoá)." in:self.view];
        return;
    }
    if (![GMVal(s, @"ok") boolValue]) {
        [GMTheme toast:[NSString stringWithFormat:@"Định dạng %@ chưa phát được trên iOS 5 / 6.", [GMVal(s, @"ext") uppercaseString]]
                    in:self.view];
        return;
    }
    [self playFrom:g song:s shuffle:[GMPlayer shared].shuffle];
}

/// Phát cả danh sách đang xem, bắt đầu ở bài `g`.
- (void)playFrom:(NSInteger)g song:(NSDictionary *)s shuffle:(BOOL)shuffle {
    GMPlayer *p = [GMPlayer shared];
    if (!_list) {
        [p playKeys:[_shown valueForKey:@"key"] at:g info:[NSDictionary dictionaryWithObjects:_shown forKeys:[_shown valueForKey:@"key"]]
            shuffle:shuffle];
        return;
    }
    NSString *key = GMVal(s, @"key");
    [_list allKeys:^(NSArray *keys) {
        NSInteger at = (g < (NSInteger)keys.count && [[keys objectAtIndex:g] isEqual:key]) ? g : (NSInteger)[keys indexOfObject:key];
        if (!key || at == NSNotFound) {
            if (s) [p playSongs:@[s] at:0];
            return;
        }
        [p playKeys:keys at:at info:[_list knownInfo] shuffle:shuffle];
    }];
}

- (void)playAll {
    NSDictionary *s = [self songAt:0];
    if (_list && !s) return;
    [self playFrom:0 song:s shuffle:NO];
}

- (void)shuffleAll {
    NSInteger n = _list ? _list.total : (NSInteger)_shown.count;
    if (!n) return;
    if (!_list) {
        [self playFrom:arc4random_uniform((u_int32_t)n) song:nil shuffle:YES];
        return;
    }
    [_list allKeys:^(NSArray *keys) {
        if (keys.count) [[GMPlayer shared] playKeys:keys at:arc4random_uniform((u_int32_t)keys.count) info:[_list knownInfo] shuffle:YES];
    }];
}

#pragma mark - Menu «⋯», lưu thành playlist, chọn nhiều bài

- (NSInteger)count {
    return _list ? _list.total : (NSInteger)_shown.count;
}

/// Bảng thao tác dùng khối (UIActionSheet không giữ khối — lưu ở `_menu`).
- (void)sheet:(NSString *)title items:(NSArray *)items from:(UIBarButtonItem *)anchor {
    UIActionSheet *a = [[UIActionSheet alloc] initWithTitle:title delegate:self cancelButtonTitle:nil destructiveButtonTitle:nil
                                          otherButtonTitles:nil];
    _menu = [NSMutableArray array];
    for (NSArray *it in items) {
        [a addButtonWithTitle:[it objectAtIndex:0]];
        [_menu addObject:[it objectAtIndex:1]];
    }
    a.cancelButtonIndex = [a addButtonWithTitle:@"Huỷ"];
    if (UI_USER_INTERFACE_IDIOM() == UIUserInterfaceIdiomPad && anchor) [a showFromBarButtonItem:anchor animated:YES];
    else [a showInView:self.view.window ?: self.view];
}

- (void)actionSheet:(UIActionSheet *)sheet clickedButtonAtIndex:(NSInteger)i {
    if (i >= 0 && i < (NSInteger)_menu.count) ((void (^)(void))[_menu objectAtIndex:i])();
}

- (void)showMenu {
    __weak GMSongsViewController *ws = self;
    NSMutableArray *items = [NSMutableArray array];
    if (_kind == GMSongsLibrary || _kind == GMSongsSource) [items addObject:@[@"Lấy bài mới", [^{ [ws refresh]; } copy]]];
    if (_forum && !_topic) [items addObject:@[@"Xem theo chủ đề", [^{ [ws openTopics]; } copy]]];
    NSInteger n = [self count];
    if (n > 0) {
        NSString *t = [NSString stringWithFormat:@"Lưu %@ thành playlist (%ld bài)", _search.text.length ? @"kết quả tìm" : @"danh sách",
                       (long)MIN(n, 5000)];
        [items addObject:@[t, [^{ [ws saveAsPlaylist]; } copy]]];
        [items addObject:@[@"Chọn nhiều bài…", [^{ [ws startSelecting]; } copy]]];
    }
    [self sheet:nil items:items from:self.navigationItem.rightBarButtonItem];
}

- (void)openTopics {
    GMTopicsViewController *t = [[GMTopicsViewController alloc] initWithStyle:UITableViewStylePlain];
    t.peer = _source;
    t.title = self.title;
    [self.navigationController pushViewController:t animated:YES];
}

/// Tên gợi ý: chủ đề / nguồn / từ tìm / tên màn.
- (NSString *)suggestedName {
    NSString *q = [_search.text stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    if (q.length) return q;
    return self.title.length ? self.title : @"Playlist mới";
}

- (void)saveAsPlaylist {
    UIView *host = self.view;
    GMAskText(@"Lưu thành playlist", @"Tên playlist", [self suggestedName], ^(NSString *name) {
        void (^done)(id, NSDictionary *) = ^(id ok, NSDictionary *err) {
            if (err) {
                [GMTheme toast:GMErrorText(err) in:host];
                return;
            }
            [GMTheme toast:[NSString stringWithFormat:@"Đã lưu «%@» (%@ bài)", name, GMVal(ok, @"count") ?: GMVal(ok, @"added")] in:host];
        };
        if (_list) {
            NSMutableDictionary *a = [[_list queryArgs] mutableCopy];
            [a setObject:name forKey:@"name"];
            [GMCore call:@"pl.fromQuery" args:a done:done];
        } else {
            [GMCore call:@"pl.create" args:@{@"name": name, @"keys": [_shown valueForKey:@"key"]} done:^(id ok, NSDictionary *err) {
                done(err ? nil : @{@"count": @(_shown.count)}, err);
            }];
        }
    });
}

- (void)startSelecting {
    if (_selecting) return;
    _selecting = YES;
    [_search resignFirstResponder];
    _savedLeft = self.navigationItem.leftBarButtonItem;
    _savedRight = self.navigationItem.rightBarButtonItem;
    self.navigationItem.hidesBackButton = YES;
    self.tableView.allowsMultipleSelectionDuringEditing = YES;
    [self setEditing:YES animated:YES];
    self.navigationItem.rightBarButtonItem =
        [[UIBarButtonItem alloc] initWithTitle:@"Xong" style:UIBarButtonItemStyleDone target:self action:@selector(stopSelecting)];
    self.navigationItem.leftBarButtonItem =
        [[UIBarButtonItem alloc] initWithTitle:@"Thao tác" style:UIBarButtonItemStyleBordered target:self action:@selector(selectionMenu)];
    [self updateSelectButtons];
    [GMTheme toast:@"Chạm để chọn bài" in:self.view];
}

- (void)stopSelecting {
    _selecting = NO;
    [self setEditing:NO animated:YES];
    self.navigationItem.leftBarButtonItem = _savedLeft;
    self.navigationItem.rightBarButtonItem = _savedRight;
    self.navigationItem.hidesBackButton = NO;
    [self.tableView reloadData];
}

- (NSArray *)selectedSongs {
    NSMutableArray *v = [NSMutableArray array];
    NSArray *ips = [[self.tableView indexPathsForSelectedRows] sortedArrayUsingSelector:@selector(compare:)];
    for (NSIndexPath *ip in ips) {
        NSDictionary *s = [self songAt:[self globalIndex:ip]];
        if (s && ![GMVal(s, @"missing") boolValue]) [v addObject:s];
    }
    return v;
}

- (void)updateSelectButtons {
    NSUInteger n = [self.tableView indexPathsForSelectedRows].count;
    self.navigationItem.leftBarButtonItem.title = n ? [NSString stringWithFormat:@"Thao tác (%lu)", (unsigned long)n] : @"Thao tác";
    self.navigationItem.leftBarButtonItem.enabled = n > 0;
}

- (void)selectionMenu {
    NSArray *songs = [self selectedSongs];
    if (!songs.count) return;
    NSArray *keys = [songs valueForKey:@"key"];
    NSMutableArray *playable = [NSMutableArray array];
    for (NSDictionary *s in songs) if ([GMVal(s, @"ok") boolValue]) [playable addObject:s];
    __weak GMSongsViewController *ws = self;
    UIView *host = self.view;
    NSUInteger n = songs.count;
    NSMutableArray *items = [NSMutableArray array];
    if (playable.count) {
        [items addObject:@[[NSString stringWithFormat:@"Phát %lu bài", (unsigned long)playable.count], [^{
            [[GMPlayer shared] playSongs:playable at:0];
            [ws stopSelecting];
        } copy]]];
        [items addObject:@[@"Phát tiếp theo", [^{
            // chen theo đúng thứ tự: bài cuối chen trước
            for (NSDictionary *s in [playable reverseObjectEnumerator]) [[GMPlayer shared] insertNext:s];
            [GMTheme toast:@"Sẽ phát tiếp theo" in:host];
            [ws stopSelecting];
        } copy]]];
    }
    [items addObject:@[@"Thêm vào playlist…", [^{
        [GMSongActions addKeys:keys toPlaylistFrom:ws];
        [ws stopSelecting];
    } copy]]];
    [items addObject:@[[NSString stringWithFormat:@"Tạo playlist mới từ %lu bài", (unsigned long)n], [^{
        GMAskText(@"Playlist mới", @"Tên playlist", [ws suggestedName], ^(NSString *name) {
            [GMCore call:@"pl.create" args:@{@"name": name, @"keys": keys} done:^(id ok, NSDictionary *err) {
                [GMTheme toast:err ? GMErrorText(err) : [NSString stringWithFormat:@"Đã tạo «%@» (%lu bài)", name, (unsigned long)n] in:host];
            }];
            [ws stopSelecting];
        });
    } copy]]];
    [items addObject:@[[NSString stringWithFormat:@"Tải về máy (%lu bài)", (unsigned long)n], [^{
        [GMCore call:@"dl.add" args:@{@"keys": keys} done:^(id ok, NSDictionary *err) {
            [GMTheme toast:err ? GMErrorText(err) : [NSString stringWithFormat:@"Đang tải %@ bài về máy…", GMVal(ok, @"added")] in:host];
        }];
        [ws stopSelecting];
    } copy]]];
    [self sheet:[NSString stringWithFormat:@"%lu bài đã chọn", (unsigned long)n] items:items from:self.navigationItem.leftBarButtonItem];
}

#pragma mark - Tìm, xếp, làm mới

- (void)searchBar:(UISearchBar *)bar textDidChange:(NSString *)text {
    [_searchTimer invalidate];
    _searchTimer = [NSTimer scheduledTimerWithTimeInterval:0.3 target:self selector:@selector(runSearch) userInfo:nil repeats:NO];
}

- (void)runSearch {
    if (_list) {
        _list.query = _search.text;
        [_list reload];
    } else {
        [self filterItems];
    }
}

- (void)searchBarSearchButtonClicked:(UISearchBar *)bar {
    [bar resignFirstResponder];
}

- (void)scrollViewWillBeginDragging:(UIScrollView *)sv {
    [_search resignFirstResponder];
}

- (void)sortChanged {
    NSArray *sorts = @[@"date", @"title", @"artist"];
    _list.sort = [sorts objectAtIndex:MAX(0, _sortCtl.selectedSegmentIndex)];
    if (_kind != GMSongsOffline) [[NSUserDefaults standardUserDefaults] setObject:_list.sort forKey:@"songs.sort"];
    [_list reload];
}

- (void)refresh {
    NSDictionary *a = _kind == GMSongsSource && _source ? @{@"peer": _source} : @{};
    [GMCore call:@"sync.start" args:a done:^(id ok, NSDictionary *err) {
        if (err) [GMTheme toast:GMErrorText(err) in:self.view];
        else if (![GMVal(ok, @"started") boolValue]) [GMTheme toast:@"Đang quét rồi." in:self.view];
        else [GMTheme toast:@"Đang tìm bài mới…" in:self.view];
        GM_IOS6_BEGIN
        if (_refresh) [(UIRefreshControl *)_refresh endRefreshing];
        GM_IOS6_END
    }];
}

- (void)pulled {
    [self refresh];
}

#pragma mark - Sự kiện

- (void)scheduleReload {
    if (!_list) return;
    [_reloadTimer invalidate];
    _reloadTimer = [NSTimer scheduledTimerWithTimeInterval:1.0 target:_list selector:@selector(reload) userInfo:nil repeats:NO];
}

- (void)coreEvent:(NSNotification *)n {
    NSString *e = GMVal(n.userInfo, @"e");
    if ([e isEqual:@"library.changed"] || [e isEqual:@"cache.cleared"]) {
        [self scheduleReload];
        if (_kind == GMSongsPlaylist && [e isEqual:@"cache.cleared"]) [self loadItems];
    } else if ([e isEqual:@"sync.state"]) {
        _sync = n.userInfo;
        [self updateStatus];
        if (![GMVal(_sync, @"running") boolValue]) [self scheduleReload];
    } else if ([e isEqual:@"dl.progress"]) {
        NSString *key = GMVal(n.userInfo, @"key");
        double total = [GMVal(n.userInfo, @"total") doubleValue];
        if (key && total > 0) [_progress setObject:@([GMVal(n.userInfo, @"have") doubleValue] / total) forKey:key];
        [self reloadVisible];
    } else if ([e isEqual:@"dl.done"] || [e isEqual:@"stream.done"] || [e isEqual:@"dl.error"]) {
        NSString *key = GMVal(n.userInfo, @"key");
        if (key) [_progress removeObjectForKey:key];
        if (_kind == GMSongsOffline) [self scheduleReload];
        else if (_list) {
            NSDictionary *s = [[_list knownInfo] objectForKey:key];
            if (s && ![e isEqual:@"dl.error"]) {
                NSMutableDictionary *m = [s mutableCopy];
                NSString *dl = GMVal(s, @"dl");
                [m setObject:([dl isEqual:@"queued"] || [e isEqual:@"dl.done"]) ? @"offline" : @"cached" forKey:@"dl"];
                [_list replaceSong:m];
            }
            [self reloadVisible];
        } else {
            [self loadItems];
        }
        if ([e isEqual:@"dl.error"] && self.view.window) [GMTheme toast:GMErrorText(n.userInfo) in:self.view];
    }
}

- (void)playerChanged {
    NSString *k = GMVal([GMPlayer shared].current, @"key");
    if ((k || _curKey) && ![k isEqual:_curKey]) {
        _curKey = k;
        [self reloadVisible];
    }
}

@end
