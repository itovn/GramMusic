#import "GMLocalViewController.h"
#import "GMCore.h"
#import "GMLocalMusic.h"
#import "GMLog.h"
#import "GMPlayer.h"
#import "GMSongCell.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

static UIButton *GMLocalButton(NSString *title, id target, SEL sel) {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeCustom];
    [b setTitle:title forState:UIControlStateNormal];
    [b setTitleColor:[GMTheme accent] forState:UIControlStateNormal];
    [b setTitleColor:[[GMTheme accent] colorWithAlphaComponent:0.4] forState:UIControlStateHighlighted];
    b.titleLabel.font = [UIFont boldSystemFontOfSize:13];
    b.layer.cornerRadius = 6;
    b.layer.borderWidth = 1;
    b.layer.borderColor = [[GMTheme accent] colorWithAlphaComponent:0.6].CGColor;
    b.backgroundColor = [GMTheme glassFill];
    [b addTarget:target action:sel forControlEvents:UIControlEventTouchUpInside];
    return b;
}

/// Chân bảng nhiều dòng (chữ xám giữa), hoặc khung rỗng (ẩn dòng kẻ thừa).
static UIView *GMLocalFooter(UITableView *tv, NSString *text) {
    CGFloat w = tv.bounds.size.width;
    UIView *v = [[UIView alloc] initWithFrame:CGRectMake(0, 0, w, text ? 84 : 0)];
    if (text) {
        UILabel *l = [[UILabel alloc] initWithFrame:CGRectMake(16, 8, w - 32, 68)];
        l.text = text;
        l.numberOfLines = 0;
        l.textAlignment = GMAlignCenter;
        l.font = [UIFont systemFontOfSize:13];
        l.textColor = [GMTheme secondaryText];
        l.backgroundColor = [UIColor clearColor];
        l.autoresizingMask = UIViewAutoresizingFlexibleWidth;
        [v addSubview:l];
    }
    return v;
}

/// Bảng «⋯» của bài trên máy: chỉ các việc với hàng chờ (Yêu thích / Playlist / Tải về dành cho bài trên Telegram).
@interface GMLocalMore : NSObject <UIActionSheetDelegate>
@property (nonatomic, strong) NSDictionary *song;
@end

static GMLocalMore *gMore;

@implementation GMLocalMore
+ (void)showFor:(NSDictionary *)song in:(UIView *)view {
    gMore = [[GMLocalMore alloc] init];
    gMore.song = song;
    UIActionSheet *a = [[UIActionSheet alloc] initWithTitle:GMVal(song, @"title") delegate:gMore cancelButtonTitle:nil
                                     destructiveButtonTitle:nil otherButtonTitles:@"Phát tiếp theo", @"Thêm vào hàng chờ", nil];
    a.cancelButtonIndex = [a addButtonWithTitle:@"Huỷ"];
    [a showInView:view.window ?: view];
}
- (void)actionSheet:(UIActionSheet *)a clickedButtonAtIndex:(NSInteger)i {
    if (i == 0) [[GMPlayer shared] insertNext:_song];
    else if (i == 1) [[GMPlayer shared] append:_song];
}
- (void)actionSheet:(UIActionSheet *)a didDismissWithButtonIndex:(NSInteger)i {
    gMore = nil;
}
@end

#pragma mark - Nhạc của app Nhạc

@interface GMIPodSongsViewController () <UISearchBarDelegate>
@end

@implementation GMIPodSongsViewController {
    NSArray *_all;
    NSArray *_shown;
    NSString *_q;
    NSString *_curKey;
    BOOL _loading;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Nhạc trên máy";
    self.tableView.rowHeight = [GMSongCell height];
    CGFloat w = self.tableView.bounds.size.width;
    UIView *h = [[UIView alloc] initWithFrame:CGRectMake(0, 0, w, 88)];
    UISearchBar *sb = [[UISearchBar alloc] initWithFrame:CGRectMake(0, 0, w, 44)];
    sb.placeholder = @"Tìm bài, ca sĩ, album";
    sb.delegate = self;
    sb.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [h addSubview:sb];
    UIButton *all = GMLocalButton(@"Phát hết", self, @selector(playAll));
    all.frame = CGRectMake(12, 50, 120, 30);
    [h addSubview:all];
    UIButton *mix = GMLocalButton(@"Ngẫu nhiên", self, @selector(shuffleAll));
    mix.frame = CGRectMake(140, 50, 120, 30);
    [h addSubview:mix];
    self.tableView.tableHeaderView = h;
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(playerChanged) name:GMPlayerChangedNotification object:nil];
    _loading = YES;
    self.tableView.tableFooterView = GMLocalFooter(self.tableView, @"Đang đọc thư viện…");
    [GMLocalMusic loadIPodSongs:^(NSArray *songs) {
        _loading = NO;
        _all = songs;
        [self filter];
    }];
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

- (void)filter {
    if (!_q.length) {
        _shown = _all;
    } else {
        NSStringCompareOptions o = NSCaseInsensitiveSearch | NSDiacriticInsensitiveSearch;
        NSMutableArray *m = [NSMutableArray array];
        for (NSDictionary *s in _all) {
            for (NSString *f in @[@"title", @"artist", @"album"]) {
                if ([GMVal(s, f) rangeOfString:_q options:o].location != NSNotFound) {
                    [m addObject:s];
                    break;
                }
            }
        }
        _shown = m;
    }
    NSString *note = nil;
    if (!_all.count) {
        note = @"App Nhạc chưa có bài nào. Chép nhạc vào máy bằng iTunes (đồng bộ) — hoặc dùng «Tệp trên máy» cho tệp chép bằng iFile / "
               @"Filza (cả FLAC, OGG, Opus).";
    } else if (!_shown.count) {
        note = @"Không thấy bài nào.";
    } else {
        note = [NSString stringWithFormat:@"%lu bài", (unsigned long)_shown.count];
    }
    self.tableView.tableFooterView = GMLocalFooter(self.tableView, note);
    [self.tableView reloadData];
}

- (void)searchBar:(UISearchBar *)sb textDidChange:(NSString *)text {
    _q = [text stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    [self filter];
}

- (void)searchBarSearchButtonClicked:(UISearchBar *)sb {
    [sb resignFirstResponder];
}

- (void)playerChanged {
    // Chỉ vẽ lại khi đổi bài (tô màu bài đang phát) — trình phát báo mỗi giây
    NSString *k = GMVal([GMPlayer shared].current, @"key");
    if ((k || _curKey) && ![k isEqual:_curKey]) {
        _curKey = k;
        [self.tableView reloadData];
    }
}

- (void)play:(NSInteger)i shuffle:(BOOL)shuffle {
    if (!_shown.count) return;
    NSMutableArray *keys = [NSMutableArray arrayWithCapacity:_shown.count];
    NSMutableDictionary *info = [NSMutableDictionary dictionaryWithCapacity:_shown.count];
    for (NSDictionary *s in _shown) {
        NSString *k = GMVal(s, @"key");
        [keys addObject:k];
        [info setObject:s forKey:k];
    }
    [[GMPlayer shared] playKeys:keys at:i info:info shuffle:shuffle];
}

- (void)playAll {
    [self play:0 shuffle:NO];
}

- (void)shuffleAll {
    [self play:(NSInteger)arc4random_uniform((uint32_t)MAX(1, _shown.count)) shuffle:YES];
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return _shown.count;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    GMSongCell *c = (GMSongCell *)[tv dequeueReusableCellWithIdentifier:@"song"];
    if (!c) c = [[GMSongCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:@"song"];
    NSDictionary *s = [_shown objectAtIndex:ip.row];
    NSString *cur = GMVal([GMPlayer shared].current, @"key");
    [c setSong:s current:[cur isEqual:GMVal(s, @"key")] progress:-1 showSource:NO];
    __weak UIView *v = self.view;
    c.more = ^{ [GMLocalMore showFor:s in:v]; };
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    NSDictionary *s = [_shown objectAtIndex:ip.row];
    if (![GMVal(s, @"ok") boolValue]) {
        [GMTheme toast:@"Bài này không phát được ngoài app Nhạc (có DRM hoặc chưa tải về máy)." in:self.view];
        return;
    }
    [self play:ip.row shuffle:NO];
}

@end

#pragma mark - Tệp trên máy

static NSString *const kAppsMode = @"<apps>";

@implementation GMFilesViewController {
    NSString *_path;   // nil = trang vị trí; kAppsMode = Documents của các app
    NSArray *_items;   // @{name, path, dir, size?, detail?}
    NSArray *_audio;   // các tệp nhạc trong thư mục (theo thứ tự hiện)
    NSString *_note;
    BOOL _loading;
    NSString *_curKey;
}

- (instancetype)initWithPath:(NSString *)path title:(NSString *)title {
    if ((self = [super initWithStyle:UITableViewStylePlain])) {
        _path = [path copy];
        self.title = title ?: @"Tệp trên máy";
    }
    return self;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.tableView.rowHeight = 52;
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(playerChanged) name:GMPlayerChangedNotification object:nil];
    [self load];
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

- (void)playerChanged {
    NSString *k = GMVal([GMPlayer shared].current, @"key");
    if ((k || _curKey) && ![k isEqual:_curKey]) {
        _curKey = k;
        [self.tableView reloadData];
    }
}

/// Trang vị trí: chỉ những chỗ có thật trên máy.
+ (NSArray *)places {
    NSFileManager *fm = [NSFileManager defaultManager];
    NSMutableArray *out = [NSMutableArray array];
    NSArray *cands = @[
        @[@"Tài liệu", @"/var/mobile/Documents"],
        @[@"Tải về", @"/var/mobile/Media/Downloads"],
        @[@"Tài liệu của các app", kAppsMode],
        @[@"Thư mục người dùng", @"/var/mobile"],
        @[@"Toàn bộ máy", @"/"],
    ];
    for (NSArray *c in cands) {
        NSString *p = [c objectAtIndex:1];
        BOOL dir = NO;
        BOOL apps = [p isEqualToString:kAppsMode];
        if (apps || ([fm fileExistsAtPath:p isDirectory:&dir] && dir)) {
            [out addObject:@{@"name": [c objectAtIndex:0], @"path": p, @"dir": @YES,
                             @"detail": apps ? @"Documents của app đã cài (VLC, trình tải nhạc…)" : p}];
        }
    }
    return out;
}

/// App (App Store) có thư mục Documents không rỗng — hiện tên app.
+ (NSArray *)apps {
    NSFileManager *fm = [NSFileManager defaultManager];
    NSString *root = @"/var/mobile/Applications";
    NSMutableArray *out = [NSMutableArray array];
    for (NSString *uuid in [fm contentsOfDirectoryAtPath:root error:NULL]) {
        NSString *home = [root stringByAppendingPathComponent:uuid];
        NSString *docs = [home stringByAppendingPathComponent:@"Documents"];
        NSArray *inDocs = [fm contentsOfDirectoryAtPath:docs error:NULL];
        if (!inDocs.count) continue;
        NSString *name = nil;
        for (NSString *f in [fm contentsOfDirectoryAtPath:home error:NULL]) {
            if (![f hasSuffix:@".app"]) continue;
            NSDictionary *info = [NSDictionary dictionaryWithContentsOfFile:
                                                   [[home stringByAppendingPathComponent:f] stringByAppendingPathComponent:@"Info.plist"]];
            name = [info objectForKey:@"CFBundleDisplayName"] ?: [info objectForKey:@"CFBundleName"] ?: f.stringByDeletingPathExtension;
            break;
        }
        if (!name) continue;
        [out addObject:@{@"name": name, @"path": docs, @"dir": @YES,
                         @"detail": [NSString stringWithFormat:@"%lu mục", (unsigned long)inDocs.count]}];
    }
    [out sortUsingComparator:^NSComparisonResult(NSDictionary *a, NSDictionary *b) {
        return [[a objectForKey:@"name"] localizedCaseInsensitiveCompare:[b objectForKey:@"name"]];
    }];
    return out;
}

/// Thư mục + tệp nhạc phát được (bỏ tệp ẩn, tệp khác), thư mục trước, theo tên.
+ (NSArray *)listDirectory:(NSString *)path {
    NSFileManager *fm = [NSFileManager defaultManager];
    NSArray *names = [fm contentsOfDirectoryAtPath:path error:NULL];
    if (!names) return nil;
    NSMutableArray *out = [NSMutableArray array];
    for (NSString *n in names) {
        if ([n hasPrefix:@"."]) continue;
        NSString *full = [path stringByAppendingPathComponent:n];
        BOOL dir = NO;
        if (![fm fileExistsAtPath:full isDirectory:&dir]) continue;  // liên kết hỏng
        if (!dir && ![GMLocalMusic playableFile:n]) continue;
        NSMutableDictionary *it = [@{@"name": n, @"path": full, @"dir": @(dir)} mutableCopy];
        if (!dir) {
            NSDictionary *attr = [fm attributesOfItemAtPath:[full stringByResolvingSymlinksInPath] error:NULL];
            [it setObject:@([attr fileSize]) forKey:@"size"];
        }
        [out addObject:it];
    }
    [out sortUsingComparator:^NSComparisonResult(NSDictionary *a, NSDictionary *b) {
        BOOL da = [[a objectForKey:@"dir"] boolValue], db = [[b objectForKey:@"dir"] boolValue];
        if (da != db) return da ? NSOrderedAscending : NSOrderedDescending;
        return [[a objectForKey:@"name"] localizedStandardCompare:[b objectForKey:@"name"]];
    }];
    return out;
}

- (void)load {
    if (!_path) {
        _items = [GMFilesViewController places];
        [self loaded];
        return;
    }
    _loading = YES;
    [self loaded];
    NSString *path = _path;
    dispatch_async(dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_DEFAULT, 0), ^{
        BOOL apps = [path isEqualToString:kAppsMode];
        NSArray *items = apps ? [GMFilesViewController apps] : [GMFilesViewController listDirectory:path];
        dispatch_async(dispatch_get_main_queue(), ^{
            _loading = NO;
            _items = items ?: @[];
            _note = !items ? @"Không mở được thư mục này (không có quyền đọc)."
                    : items.count ? nil
                    : apps ? @"Chưa app nào có tài liệu." : @"Không có thư mục con hay tệp nhạc nào.";
            [self loaded];
        });
    });
}

- (void)loaded {
    NSMutableArray *audio = [NSMutableArray array];
    for (NSDictionary *it in _items) {
        if (![[it objectForKey:@"dir"] boolValue]) [audio addObject:it];
    }
    _audio = audio;
    NSString *foot = _loading ? @"Đang đọc…"
                     : _note ? _note
                     : !_path ? @"Tệp nhạc chép vào máy (iFile, Filza, SSH, «Chia sẻ tệp» của iTunes): mp3, m4a, wav, aiff và cả FLAC, OGG, "
                                @"Opus (GramMusic tự giải mã)."
                              : nil;
    self.tableView.tableFooterView = GMLocalFooter(self.tableView, foot);
    // Thư mục có nhạc: nút Phát hết / Ngẫu nhiên
    if (audio.count) {
        CGFloat w = self.tableView.bounds.size.width;
        UIView *h = [[UIView alloc] initWithFrame:CGRectMake(0, 0, w, 44)];
        UIButton *all = GMLocalButton([NSString stringWithFormat:@"Phát %lu bài", (unsigned long)audio.count], self, @selector(playAll));
        all.frame = CGRectMake(12, 7, 120, 30);
        [h addSubview:all];
        UIButton *mix = GMLocalButton(@"Ngẫu nhiên", self, @selector(shuffleAll));
        mix.frame = CGRectMake(140, 7, 120, 30);
        [h addSubview:mix];
        self.tableView.tableHeaderView = h;
    } else {
        self.tableView.tableHeaderView = nil;
    }
    [self.tableView reloadData];
}

- (void)playFrom:(NSInteger)i shuffle:(BOOL)shuffle {
    if (!_audio.count) return;
    NSMutableArray *keys = [NSMutableArray arrayWithCapacity:_audio.count];
    for (NSDictionary *it in _audio) [keys addObject:[@"file:" stringByAppendingString:[it objectForKey:@"path"]]];
    GMLog(@"tệp trên máy: phát %lu tệp", (unsigned long)keys.count);
    // Thông tin bài (tên, thời lượng…) lõi đọc từ tệp khi cần (GMPlayer fetchInfo → library.songs)
    [[GMPlayer shared] playKeys:keys at:i info:@{} shuffle:shuffle];
}

- (void)playAll {
    [self playFrom:0 shuffle:NO];
}

- (void)shuffleAll {
    [self playFrom:(NSInteger)arc4random_uniform((uint32_t)MAX(1, _audio.count)) shuffle:YES];
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return _items.count;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"f"];
    if (!c) {
        c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:@"f"];
        c.textLabel.lineBreakMode = GMMiddleTruncation;
    }
    c.detailTextLabel.textColor = [GMTheme secondaryText];
    NSDictionary *it = [_items objectAtIndex:ip.row];
    BOOL dir = [[it objectForKey:@"dir"] boolValue];
    NSString *name = [it objectForKey:@"name"];
    BOOL cur = !dir && [_curKey isEqual:[@"file:" stringByAppendingString:[it objectForKey:@"path"]]];
    c.textLabel.text = dir ? name : name.stringByDeletingPathExtension;
    c.textLabel.textColor = cur ? [GMTheme accent] : [GMTheme text];
    c.imageView.image = dir ? [GMTheme folderIcon] : [GMTheme fileIcon:name.pathExtension];
    c.accessoryType = dir ? UITableViewCellAccessoryDisclosureIndicator : UITableViewCellAccessoryNone;
    NSString *detail = [it objectForKey:@"detail"];
    if (!detail && !dir) {
        detail = [NSString stringWithFormat:@"%@ · %@", name.pathExtension.uppercaseString,
                                            [GMTheme sizeText:[[it objectForKey:@"size"] doubleValue]]];
    }
    c.detailTextLabel.text = detail ?: @"";
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    NSDictionary *it = [_items objectAtIndex:ip.row];
    if ([[it objectForKey:@"dir"] boolValue]) {
        [self debugOpenPath:[it objectForKey:@"path"] title:[it objectForKey:@"name"] animated:YES];
        return;
    }
    if (![[NSFileManager defaultManager] isReadableFileAtPath:[it objectForKey:@"path"]]) {
        [GMTheme toast:@"Không đọc được tệp này." in:self.view];
        return;
    }
    [self playFrom:[_audio indexOfObject:it] shuffle:NO];
}

- (void)debugOpenPath:(NSString *)path title:(NSString *)title animated:(BOOL)animated {
    GMFilesViewController *b = [[GMFilesViewController alloc] initWithPath:path title:title];
    [self.navigationController pushViewController:b animated:animated];
}

- (void)debugOpenPath:(NSString *)path {
    [self debugOpenPath:path title:path.lastPathComponent animated:NO];
}

@end
