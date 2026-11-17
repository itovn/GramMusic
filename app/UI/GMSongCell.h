#import <UIKit/UIKit.h>

/// Ô bài hát: ảnh bìa 44 pt (bài không có ảnh bìa: ô màu tạm), tên (màu chủ đạo khi đang phát), dòng phụ «ca sĩ · 3:45 ·
/// nguồn», nhãn định dạng chưa phát được (FLAC…), dấu đã tải, nút «⋯» mở bảng thao tác.
@interface GMSongCell : UITableViewCell
@property (nonatomic, copy) void (^more)(void);
/// `progress`: 0…1 đang tải về máy, < 0 = không tải. `showSource`: ghi tên nguồn ở dòng phụ.
- (void)setSong:(NSDictionary *)song current:(BOOL)current progress:(double)progress showSource:(BOOL)showSource;
/// Ô đang chờ trang dữ liệu.
- (void)setLoading;
+ (CGFloat)height;
@end
