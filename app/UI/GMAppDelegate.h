#import <UIKit/UIKit.h>

@class GMRootController;

@interface GMAppDelegate : UIResponder <UIApplicationDelegate>
@property (nonatomic, strong) UIWindow *window;
@property (nonatomic, strong) GMRootController *root;
/// Màn đăng nhập vừa đóng (đăng nhập sau).
- (void)loginClosed;
- (void)showLogin;
/// Đổi giao diện sáng / tối: dựng lại cây màn.
- (void)reloadTheme;
@end
