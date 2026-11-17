#import "GMRootController.h"
#import "GMAppDelegate.h"
#import "GMArt.h"
#import "GMCore.h"
#import "GMImages.h"
#import "GMNowPlayingViewController.h"
#import "GMPlayer.h"
#import "GMPlaylistsViewController.h"
#import "GMSettingsViewController.h"
#import "GMSongsViewController.h"
#import "GMSourcesViewController.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

GMRootController *GMRoot(void) {
    return [(GMAppDelegate *)[UIApplication sharedApplication].delegate root];
}

#pragma mark - Thanh phát nhỏ

@interface GMMiniPlayer : UIView <UIGestureRecognizerDelegate>
@property (nonatomic, copy) void (^tap)(void);
- (void)refresh;
@end

@implementation GMMiniPlayer {
    UIImageView *_bg, *_cover;
    UILabel *_title, *_sub;
    UIButton *_play, *_next;
    UIView *_bar, *_barFill;
    UIActivityIndicatorView *_spin;
    NSString *_coverKey;
}

- (instancetype)initWithFrame:(CGRect)frame {
    if ((self = [super initWithFrame:frame])) {
        _bg = [[UIImageView alloc] initWithImage:[GMTheme glassBar]];
        _bg.frame = self.bounds;
        _bg.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
        [self addSubview:_bg];
        _cover = [[UIImageView alloc] initWithFrame:CGRectMake(8, 7, 38, 38)];
        _cover.layer.cornerRadius = 4;
        _cover.clipsToBounds = YES;
        [self addSubview:_cover];
        _title = [[UILabel alloc] init];
        _title.font = [UIFont boldSystemFontOfSize:14];
        _title.textColor = [GMTheme text];
        _title.backgroundColor = [UIColor clearColor];
        [self addSubview:_title];
        _sub = [[UILabel alloc] init];
        _sub.font = [UIFont systemFontOfSize:12];
        _sub.textColor = [GMTheme secondaryText];
        _sub.backgroundColor = [UIColor clearColor];
        [self addSubview:_sub];
        _play = [UIButton buttonWithType:UIButtonTypeCustom];
        [_play addTarget:[GMPlayer shared] action:@selector(togglePlay) forControlEvents:UIControlEventTouchUpInside];
        [self addSubview:_play];
        _next = [UIButton buttonWithType:UIButtonTypeCustom];
        [_next setImage:[GMArt glyph:@"next" size:24 color:[GMTheme text]] forState:UIControlStateNormal];
        [_next addTarget:[GMPlayer shared] action:@selector(next) forControlEvents:UIControlEventTouchUpInside];
        [self addSubview:_next];
        _spin = [[UIActivityIndicatorView alloc] initWithActivityIndicatorStyle:
                 [GMTheme dark] ? UIActivityIndicatorViewStyleWhite : UIActivityIndicatorViewStyleGray];
        _spin.hidesWhenStopped = YES;
        _spin.userInteractionEnabled = NO;  // chạm xuyên xuống nút phát / dừng bên dưới
        [self addSubview:_spin];
        _bar = [[UIView alloc] init];
        _bar.backgroundColor = [[GMTheme accent] colorWithAlphaComponent:0.18];
        [self addSubview:_bar];
        _barFill = [[UIView alloc] init];
        _barFill.backgroundColor = [GMTheme accent];
        [_bar addSubview:_barFill];
        UITapGestureRecognizer *t = [[UITapGestureRecognizer alloc] initWithTarget:self action:@selector(tapped)];
        t.delegate = self;
        [self addGestureRecognizer:t];
    }
    return self;
}

/// iOS 5: chạm của view cha thắng nút con (nút Phát / Tiếp mở màn Đang phát thay vì chạy) → không nhận chạm trên nút.
- (BOOL)gestureRecognizer:(UIGestureRecognizer *)g shouldReceiveTouch:(UITouch *)touch {
    return ![touch.view isKindOfClass:[UIControl class]];
}

- (void)tapped {
    if (_tap) _tap();
}

- (void)layoutSubviews {
    [super layoutSubviews];
    CGFloat w = self.bounds.size.width, h = self.bounds.size.height;
    _play.frame = CGRectMake(w - 92, 0, 44, h);
    _spin.center = _play.center;
    _next.frame = CGRectMake(w - 48, 0, 44, h);
    _title.frame = CGRectMake(54, 8, w - 54 - 96, 18);
    _sub.frame = CGRectMake(54, 27, w - 54 - 96, 16);
    _bar.frame = CGRectMake(0, 0, w, 2);
    [self refresh];
}

- (void)refresh {
    GMPlayer *p = [GMPlayer shared];
    NSDictionary *s = p.current;
    NSString *title = GMVal(s, @"title");
    _title.text = title.length ? title : @"Chọn một bài để nghe";
    NSString *artist = GMVal(s, @"artist");
    NSString *src = GMVal(s, @"src");
    if (!p.playing && p.position > 5) _sub.text = [NSString stringWithFormat:@"Nghe tiếp từ %@", GMDuration(p.position)];
    else _sub.text = artist.length ? artist : (src.length ? src : @"");
    BOOL busy = p.playing && p.buffering;
    if (busy) [_spin startAnimating];
    else [_spin stopAnimating];
    // Đang chờ dữ liệu: vòng quay thay hình nút, nút vẫn bấm được (mạng chậm có thể chờ lâu — vẫn phải dừng được)
    [_play setImage:busy ? nil : [GMArt glyph:p.playing ? @"pause" : @"play" size:24 color:[GMTheme text]] forState:UIControlStateNormal];
    double d = p.duration;
    double f = d > 0 ? MIN(1, p.position / d) : 0;
    _barFill.frame = CGRectMake(0, 0, floor(_bar.bounds.size.width * f), 2);
    NSString *key = GMVal(s, @"key");
    if (![key isEqual:_coverKey]) {
        _coverKey = [GMVal(s, @"loading") boolValue] ? nil : key;  // bản tạm: chờ thông tin thật mới hỏi ảnh bìa
        _cover.image = key ? ([GMImages cachedCover:key size:38] ?: [GMArt placeholderFor:key size:38]) : nil;
        if (key) {
            [GMImages cover:s size:38 done:^(UIImage *img) {
                if ([_coverKey isEqual:key]) _cover.image = img;
            }];
        }
    }
}

@end

#pragma mark - Khung chính

@implementation GMRootController {
    NSArray *_navs;
    UIView *_content;
    UITabBar *_tabBar;
    GMMiniPlayer *_mini;
    UIViewController *_shown;
    GMSongsViewController *_library;
}

- (instancetype)init {
    if ((self = [super init])) {
        _library = [[GMSongsViewController alloc] initWithKind:GMSongsLibrary];
        UINavigationController *lib = [[UINavigationController alloc] initWithRootViewController:_library];
        lib.tabBarItem = [GMTheme tabItem:@"Thư viện" icon:@"library"];
        UINavigationController *src =
            [[UINavigationController alloc] initWithRootViewController:[[GMSourcesViewController alloc] initWithStyle:UITableViewStylePlain]];
        src.tabBarItem = [GMTheme tabItem:@"Nguồn" icon:@"sources"];
        UINavigationController *pl =
            [[UINavigationController alloc] initWithRootViewController:[[GMPlaylistsViewController alloc] initWithStyle:UITableViewStylePlain]];
        pl.tabBarItem = [GMTheme tabItem:@"Playlist" icon:@"playlist"];
        UINavigationController *dl =
            [[UINavigationController alloc] initWithRootViewController:[[GMSongsViewController alloc] initWithKind:GMSongsOffline]];
        dl.tabBarItem = [GMTheme tabItem:@"Đã tải" icon:@"downloads"];
        UINavigationController *st =
            [[UINavigationController alloc] initWithRootViewController:[[GMSettingsViewController alloc] initWithStyle:UITableViewStyleGrouped]];
        st.tabBarItem = [GMTheme tabItem:@"Cài đặt" icon:@"settings"];
        _navs = @[lib, src, pl, dl, st];
    }
    return self;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = [GMTheme background];
    // Hình nền sau cùng: lộ ra dưới thanh trên / thanh dưới kính
    [self.view addSubview:[[GMWallpaperView alloc] initWithFrame:self.view.bounds]];
    _content = [[UIView alloc] initWithFrame:self.view.bounds];
    _content.backgroundColor = [UIColor clearColor];
    [self.view addSubview:_content];
    _mini = [[GMMiniPlayer alloc] initWithFrame:CGRectMake(0, 0, self.view.bounds.size.width, 52)];
    __weak GMRootController *ws = self;
    _mini.tap = ^{ [ws openNowPlaying]; };
    [self.view addSubview:_mini];
    _tabBar = [[UITabBar alloc] init];
    _tabBar.delegate = self;
    NSMutableArray *items = [NSMutableArray array];
    for (UIViewController *vc in _navs) [items addObject:vc.tabBarItem];
    _tabBar.items = items;
    [self.view addSubview:_tabBar];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(playerChanged) name:GMPlayerChangedNotification object:nil];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(playerMessage:) name:GMPlayerMessageNotification object:nil];
    [self show:_selectedIndex];
}

- (void)dealloc {
    [[NSNotificationCenter defaultCenter] removeObserver:self];
}

- (BOOL)canBecomeFirstResponder {
    return YES;
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    [self becomeFirstResponder];  // nhận nút điều khiển trên màn hình khoá / tai nghe
}

- (void)remoteControlReceivedWithEvent:(UIEvent *)event {
    [[GMPlayer shared] remoteEvent:event];
}

- (void)viewWillLayoutSubviews {
    [super viewWillLayoutSubviews];
    CGRect b = self.view.bounds;
    CGFloat tabH = 49;
    BOOL mini = [GMPlayer shared].current != nil;
    CGFloat miniH = mini ? 52 : 0;
    _tabBar.frame = CGRectMake(0, b.size.height - tabH, b.size.width, tabH);
    _mini.hidden = !mini;
    _mini.frame = CGRectMake(0, b.size.height - tabH - miniH, b.size.width, 52);
    _content.frame = CGRectMake(0, 0, b.size.width, b.size.height - tabH - miniH);
    _shown.view.frame = _content.bounds;
}

- (void)playerChanged {
    BOOL want = [GMPlayer shared].current != nil;
    if (want == _mini.hidden) [self.view setNeedsLayout];
    [_mini refresh];
}

- (void)playerMessage:(NSNotification *)n {
    NSString *t = [n.userInfo objectForKey:@"text"];
    UIViewController *top = self.presentedViewController ?: self;
    if (t.length) [GMTheme toast:t in:top.view];
}

- (void)setSelectedIndex:(NSInteger)i {
    _selectedIndex = MAX(0, MIN(i, 4));
    if (self.isViewLoaded) [self show:_selectedIndex];
}

- (void)show:(NSInteger)i {
    UIViewController *vc = [_navs objectAtIndex:i];
    _tabBar.selectedItem = [_tabBar.items objectAtIndex:i];
    if (vc == _shown) return;
    if (_shown) {
        [_shown willMoveToParentViewController:nil];
        [_shown.view removeFromSuperview];
        [_shown removeFromParentViewController];
    }
    [self addChildViewController:vc];
    vc.view.frame = _content.bounds;
    vc.view.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    [_content addSubview:vc.view];
    [vc didMoveToParentViewController:self];
    _shown = vc;
}

- (void)tabBar:(UITabBar *)tabBar didSelectItem:(UITabBarItem *)item {
    NSInteger i = [tabBar.items indexOfObject:item];
    if (i == NSNotFound) return;
    UINavigationController *nav = [_navs objectAtIndex:i];
    if (i == _selectedIndex && _shown == nav) {
        [nav popToRootViewControllerAnimated:YES];  // chạm lại mục đang mở: về màn đầu
        return;
    }
    self.selectedIndex = i;
}

- (void)loggedIn:(NSDictionary *)me {
    _me = me;
    [_library loggedIn];
}

- (void)loggedOut {
    _me = nil;
}

- (void)openNowPlaying {
    if (self.presentedViewController || ![GMPlayer shared].current) return;
    UINavigationController *n = [[UINavigationController alloc] initWithRootViewController:[[GMNowPlayingViewController alloc] init]];
    [self presentViewController:n animated:YES completion:nil];
}

@end
