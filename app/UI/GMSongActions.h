#import <UIKit/UIKit.h>

/// Bảng thao tác của một bài (nút «⋯» / giữ lâu trên ô): Phát tiếp theo, Thêm vào hàng chờ, Yêu thích, Thêm vào playlist…,
/// Tải về máy / Xoá bản tải, Mở nguồn, (trong playlist) Bỏ khỏi playlist. `changed(bài mới)` khi thông tin bài đổi (Yêu
/// thích, tải về), `removed()` khi bài vừa bị bỏ khỏi playlist đang xem.
@interface GMSongActions : NSObject <UIActionSheetDelegate>
+ (void)showFor:(NSDictionary *)song
             in:(UIViewController *)vc
           from:(UIView *)anchor
       playlist:(id)playlistId
        changed:(void (^)(NSDictionary *song))changed
        removed:(void (^)(void))removed;
/// Chọn playlist rồi thêm các bài (bảng chọn + «Playlist mới…»).
+ (void)addKeys:(NSArray *)keys toPlaylistFrom:(UIViewController *)vc;
@end

/// Hỏi một dòng chữ (tên playlist, @tên kênh…) bằng UIAlertView có ô nhập (iOS 5+); bấm «Lưu» với chữ không rỗng thì gọi done.
void GMAskText(NSString *title, NSString *placeholder, NSString *initial, void (^done)(NSString *text));
