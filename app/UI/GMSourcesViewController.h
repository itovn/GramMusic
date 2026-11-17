#import <UIKit/UIKit.h>

/// Nguồn nhạc: các chat có nhạc tìm được khi quét tài khoản (Tin nhắn đã lưu đầu tiên, rồi theo số bài) — công tắc ẩn / hiện
/// trong Thư viện, chạm để xem bài của nguồn; «+» thêm kênh / nhóm công khai bằng @tên hoặc link t.me (kể cả chưa vào kênh);
/// đầu bảng: trạng thái quét, «Lấy bài mới», «Quét toàn bộ».
@interface GMSourcesViewController : UITableViewController
@end

/// Chủ đề của nhóm diễn đàn: «Tất cả bài của nhóm», rồi từng chủ đề có nhạc (số bài) — chạm để nghe riêng chủ đề; nút «Mỗi chủ đề
/// thành một playlist» tạo nhanh playlist «Nhóm · Chủ đề» cho mọi chủ đề (bấm lại thì thêm bài mới vào playlist cũ).
@interface GMTopicsViewController : UITableViewController
@property (nonatomic, copy) NSString *peer;
@end
