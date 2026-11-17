#import <UIKit/UIKit.h>

/// Màn Đang phát: ảnh bìa lớn, tên / ca sĩ / nguồn, thanh thời gian (tua), Trộn / Lùi / Phát / Tiếp / Lặp, Yêu thích, Thêm vào
/// playlist, Tải về máy, Hẹn giờ tắt, Hàng chờ, âm lượng + AirPlay (MPVolumeView).
@interface GMNowPlayingViewController : UIViewController <UIActionSheetDelegate>
@end

/// Hàng chờ: bài đang phát + các bài tiếp theo (kéo để đổi thứ tự, vuốt để bỏ, chạm để phát).
@interface GMQueueViewController : UITableViewController
@end
