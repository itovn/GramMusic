#import <UIKit/UIKit.h>

/// Nhạc của app Nhạc (bài chép từ iTunes): tìm, Phát hết / Ngẫu nhiên, chạm để phát cả danh sách.
@interface GMIPodSongsViewController : UITableViewController
@end

/// Tệp nhạc trên máy: duyệt thư mục (chỉ hiện thư mục + tệp phát được), chạm một tệp để phát cả thư mục từ tệp đó.
@interface GMFilesViewController : UITableViewController
/// `path` nil = trang các vị trí (Tài liệu, Tải về…).
- (instancetype)initWithPath:(NSString *)path title:(NSString *)title;
/// Gỡ lỗi (GMDebug): mở thẳng một thư mục.
- (void)debugOpenPath:(NSString *)path;
@end
