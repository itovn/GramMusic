#import <UIKit/UIKit.h>

typedef NS_ENUM(NSInteger, GMSongsKind) {
    /// Thư viện: bài của mọi nguồn đang bật.
    GMSongsLibrary,
    /// Bài của một nguồn (`source`).
    GMSongsSource,
    /// Đã tải về máy.
    GMSongsOffline,
    /// Một playlist / Yêu thích / Nghe gần đây (`playlistId`: số, @"fav", @"recent").
    GMSongsPlaylist,
};

/// Danh sách bài: ô tìm (không dấu), xếp Mới nhất / Tên A–Z / Ca sĩ + thanh chữ cái, «Phát hết» / «Ngẫu nhiên», chạm bài để
/// phát, nút «⋯» mở bảng thao tác. Thư viện / nguồn / đã tải lấy từng trang từ lõi (GMSongList); playlist lấy cả danh sách.
@interface GMSongsViewController : UITableViewController <UISearchBarDelegate, UIActionSheetDelegate>
- (instancetype)initWithKind:(GMSongsKind)kind;
@property (nonatomic, readonly) GMSongsKind kind;
@property (nonatomic, copy) NSString *source;
@property (nonatomic, strong) id playlistId;
/// Chủ đề của nhóm diễn đàn (GMSongsSource): chỉ bài của chủ đề này.
@property (nonatomic, strong) NSNumber *topic;
/// Nhóm có chủ đề: menu có «Xem theo chủ đề».
@property (nonatomic) BOOL forum;
/// Vừa đăng nhập: tải lại thư viện.
- (void)loggedIn;
@end
