#import <UIKit/UIKit.h>

/// UIAlertView gọi khối khi bấm nút (iOS 5 / 6 chưa có UIAlertController). Nút huỷ = 0, nút `other` = 1.
@interface GMBlockAlert : UIAlertView <UIAlertViewDelegate>
@property (nonatomic, copy) void (^done)(NSInteger index);
@end

/// Hiện hộp thoại; `done(i)` khi bấm nút i (có thể nil).
GMBlockAlert *GMAlertShow(NSString *title, NSString *msg, NSString *cancel, NSString *other, void (^done)(NSInteger index));
