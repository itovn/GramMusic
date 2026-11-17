#import <UIKit/UIKit.h>

/// Khung chính: thanh dưới (Thư viện / Nguồn / Playlist / Đã tải / Cài đặt — như MeloVault), thanh phát nhỏ nằm ngay trên
/// thanh dưới (chạm để mở màn Đang phát). Tự làm thay UITabBarController (iOS 5 có API chứa màn con — addChildViewController)
/// để đặt được thanh phát nhỏ giữa nội dung và thanh dưới.
@interface GMRootController : UIViewController <UITabBarDelegate>
@property (nonatomic) NSInteger selectedIndex;
/// Tài khoản đang đăng nhập (nil = chưa).
@property (nonatomic, readonly) NSDictionary *me;
- (void)loggedIn:(NSDictionary *)me;
- (void)loggedOut;
/// Mở màn Đang phát (từ thanh phát nhỏ, hoặc sau khi chọn bài).
- (void)openNowPlaying;
@end

/// Khung chính đang dùng (để màn con mở màn Đang phát, đổi mục).
GMRootController *GMRoot(void);
