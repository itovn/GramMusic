#import <UIKit/UIKit.h>

/// Proxy: dán link tg://proxy… (MTProxy) hoặc tg://socks… (SOCKS5), thử kết nối, lưu; hoặc đi thẳng.
@interface GMProxyViewController : UIViewController <UITextFieldDelegate>
/// Link điền sẵn (mở từ tin nhắn): điền vào ô và tự thử kết nối; người dùng bấm «Lưu» để dùng.
@property (nonatomic, copy) NSString *initialLink;
@end
