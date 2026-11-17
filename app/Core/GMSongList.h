#import <Foundation/Foundation.h>

/// Danh sách bài lấy từng trang 100 bài từ lõi (`library.query`) — thư viện có thể hàng chục nghìn bài, máy A4 chỉ 256 MB RAM.
/// Bảng hỏi `songAt:`; trang chưa có thì trả nil và tự tải, tải xong gọi `changed`.
@interface GMSongList : NSObject
/// Từ cần tìm (bỏ dấu ở lõi), nguồn (khoá peer, nil = mọi nguồn), cách xếp (date | title | artist), lọc (all | offline | cached).
@property (nonatomic, copy) NSString *query;
@property (nonatomic, copy) NSString *source;
@property (nonatomic, copy) NSString *sort;
@property (nonatomic, copy) NSString *filter;
/// Chủ đề của nhóm diễn đàn (nil = mọi bài; 1 = General).
@property (nonatomic, strong) NSNumber *topic;
@property (nonatomic, readonly) NSInteger total;
/// Mục chữ cái (xếp theo tên / ca sĩ): [{t: "A", i: vị trí đầu, n: số bài}]; rỗng khi xếp theo ngày.
@property (nonatomic, readonly) NSArray *sections;
/// Đã có trang đầu (lần tải đầu xong).
@property (nonatomic, readonly) BOOL loaded;
@property (nonatomic, copy) void (^changed)(void);

/// Bỏ các trang đã có, tải lại trang đầu (đổi từ tìm / cách xếp, thư viện đổi).
- (void)reload;
- (NSDictionary *)songAt:(NSInteger)i;
/// Thay thông tin một bài đã có trong trang (Yêu thích, đã tải…).
- (void)replaceSong:(NSDictionary *)song;
/// Khoá của mọi bài (tối đa 5000 — «Phát hết» / «Ngẫu nhiên»).
- (void)allKeys:(void (^)(NSArray *keys))done;
/// Tham số truy vấn hiện tại (q, source, topic, sort, filter) — `pl.fromQuery` lưu cả danh sách thành playlist.
- (NSDictionary *)queryArgs;
/// Thông tin các bài đã có trong trang (khoá → dict) — đưa cho trình phát khỏi hỏi lại.
- (NSDictionary *)knownInfo;
@end
