#import "GMAppDelegate.h"
#import "GMCompat.h"
#import "GMCore.h"
#import "GMLog.h"
#import "GMLoginViewController.h"
#import "GMLockState.h"
#import "GMPlayer.h"
#import "GMRootController.h"
#import "GMTheme.h"
#import "GMUpdate.h"
#ifdef DEBUG
#import "GMDebug.h"
#endif

@interface GMAppDelegate () <UIAlertViewDelegate>
@end

@implementation GMAppDelegate {
    BOOL _loginShown, _deadShown;
}

- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)launchOptions {
    NSString *version = [[NSBundle mainBundle] objectForInfoDictionaryKey:@"CFBundleShortVersionString"];
    GMLog(@"=== GramMusic %@ — iOS %@%@, RAM %.0f MB", version, [UIDevice currentDevice].systemVersion,
          GMOldIOS() ? @" (kiểu iOS 5)" : @"", [NSProcessInfo processInfo].physicalMemory / 1048576.0);
    [GMTheme apply];
    BOOL started = [GMCore start];
    [[GMPlayer shared] setup];
    [GMLockState start];  // tweak GramLockControl (màn hình khoá) — M20

    self.window = [[UIWindow alloc] initWithFrame:[UIScreen mainScreen].bounds];
    [self buildUI];
    [self.window makeKeyAndVisible];
    // Nút trên màn hình khoá / tai nghe (app phải là «app đang phát» — đã có phiên âm thanh Playback)
    [application beginReceivingRemoteControlEvents];

    if (!started) {
        [[[UIAlertView alloc] initWithTitle:@"Không mở được lõi" message:@"Xem app.log trong ~/Library/GramMusic." delegate:nil
                          cancelButtonTitle:@"OK" otherButtonTitles:nil] show];
        return YES;
    }
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(event:) name:GMCoreEventNotification object:nil];
    [self checkAuth];
    [GMUpdate start];  // kiểm bản mới (OTA — M26) sau 10 giây, khi quay lại app tối đa 6 giờ một lần
#ifdef DEBUG
    [GMDebug start];
#endif
    return YES;
}

- (void)checkAuth {
    [GMCore call:@"auth.state" args:nil done:^(id ok, NSDictionary *err) {
        if (err) {
            // Mạng lỗi (bị chặn?): vẫn vào thư viện (bài đã quét / đã tải vẫn nghe được); Cài đặt → Proxy
            GMLog(@"auth.state lỗi: %@", [err objectForKey:@"code"]);
            return;
        }
        if ([GMVal(ok, @"authorized") boolValue]) {
            [self.root loggedIn:GMVal(ok, @"me")];
        } else if ([GMVal(ok, @"dead") boolValue]) {
            [self showDead];
        } else {
            [self showLogin];
        }
    }];
}

- (void)loginClosed {
    _loginShown = NO;
}

- (void)showLogin {
    if (_loginShown) return;
    _loginShown = YES;
    UINavigationController *n = [[UINavigationController alloc] initWithRootViewController:[[GMLoginViewController alloc] init]];
    [self.root presentViewController:n animated:YES completion:nil];
}

/// Telegram huỷ phiên (dùng trùng ở hai nơi / bị thu hồi — GramMini D24): lõi đã dời tệp phiên → mở lại app để đăng nhập.
- (void)showDead {
    if (_deadShown) return;
    _deadShown = YES;
    GMLog(@"phiên bị Telegram huỷ — chờ mở lại app để đăng nhập");
    [[[UIAlertView alloc] initWithTitle:@"Cần đăng nhập lại"
                                message:@"Telegram đã huỷ phiên đăng nhập của GramMusic trên máy này. Tài khoản của bạn vẫn bình "
                                        @"thường. Bấm «Mở lại» rồi chạm biểu tượng GramMusic để đăng nhập."
                               delegate:self cancelButtonTitle:nil otherButtonTitles:@"Mở lại", nil] show];
}

- (void)alertView:(UIAlertView *)alertView didDismissWithButtonIndex:(NSInteger)buttonIndex {
    if (_deadShown) exit(0);
}

- (void)event:(NSNotification *)n {
    NSString *e = GMVal(n.userInfo, @"e");
    if ([e isEqual:@"auth.dead"]) {
        [self showDead];
        return;
    }
    if (![e isEqual:@"auth.state"] || [GMVal(n.userInfo, @"dead") boolValue]) return;
    if ([GMVal(n.userInfo, @"authorized") boolValue]) {
        if (!_loginShown) return;  // lúc mở app: checkAuth đã lo
        _loginShown = NO;
        [GMCore call:@"auth.state" args:nil done:^(id ok, NSDictionary *err) {
            [self.root loggedIn:GMVal(ok, @"me")];
        }];
    } else {
        [self.root loggedOut];
        [self showLogin];
    }
}

- (void)buildUI {
    self.window.backgroundColor = [GMTheme background];
    NSInteger tab = self.root ? self.root.selectedIndex : 0;
    NSDictionary *me = self.root.me;
    self.root = [[GMRootController alloc] init];
    self.root.selectedIndex = tab;
    self.window.rootViewController = self.root;
    if (me) [self.root loggedIn:me];
}

- (void)reloadTheme {
    // Màn đang bật lên (Đang phát, Đăng nhập) thuộc khung cũ: đóng trước khi thay khung, rồi mở lại màn đăng nhập nếu cần
    BOOL login = _loginShown;
    if (self.root.presentedViewController) [self.root dismissViewControllerAnimated:NO completion:nil];
    _loginShown = NO;
    [GMTheme apply];
    [self buildUI];
    if (login) [self showLogin];
}

/// Màn hình khoá / tai nghe: sự kiện tới app khi không có view nào nhận (cuối chuỗi responder là delegate của app).
- (void)remoteControlReceivedWithEvent:(UIEvent *)event {
    [[GMPlayer shared] remoteEvent:event];
}

- (void)applicationDidBecomeActive:(UIApplication *)application {
    if ([GMTheme wantsDark] != [GMTheme dark]) [self reloadTheme];
    [GMCore call:@"net.check" args:nil done:nil];
    // Nằm nền lâu (tạm dừng → app bị treo): iOS thu hồi cổng phát 127.0.0.1 → mở lại trước khi trình phát đọc (M23)
    [GMCore call:@"stream.revive" args:@{} done:^(id ok, NSDictionary *err) {
        if ([[ok objectForKey:@"revived"] boolValue]) {
            GMLog(@"phát: cổng phát bị iOS thu hồi lúc nằm nền — đã mở lại cổng %@", [ok objectForKey:@"port"]);
        }
    }];
}

- (void)applicationDidEnterBackground:(UIApplication *)application {
    [GMCore call:@"app.flush" args:nil done:nil];
    [[GMPlayer shared] didEnterBackground];
}

- (void)applicationWillEnterForeground:(UIApplication *)application {
    [[GMPlayer shared] willEnterForeground];
}

- (void)applicationWillTerminate:(UIApplication *)application {
    [[GMPlayer shared] saveState];
    [GMCore call:@"app.flush" args:nil done:nil];
}

@end
