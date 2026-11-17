#import <UIKit/UIKit.h>

/// Đăng nhập: số điện thoại → mã Telegram gửi → mật khẩu 2 lớp (nếu có). Có nút Proxy (Telegram có thể bị chặn).
@interface GMLoginViewController : UIViewController <UITextFieldDelegate>
/// Gỡ lỗi: hiện bước 0 số điện thoại / 1 mã / 2 mật khẩu (không gọi mạng).
- (void)debugStep:(NSInteger)step;
@end
