#import <UIKit/UIKit.h>

typedef NS_ENUM(NSInteger, GMRepeatMode) { GMRepeatOff = 0, GMRepeatAll = 1, GMRepeatOne = 2 };

/// Bài / trạng thái phát / hàng chờ đổi (luồng chính). Màn Đang phát, thanh phát nhỏ, danh sách bài nghe thông báo này.
extern NSString *const GMPlayerChangedNotification;
/// Câu báo ngắn cho người dùng (bài không phát được, bỏ qua bài…): userInfo @{@"text"}.
extern NSString *const GMPlayerMessageNotification;

/// Trình phát: AVQueuePlayer đọc nhạc từ cổng phát nội bộ của lõi (`play.url` → http://127.0.0.1:…, có Range — tua được cả
/// khi bài chưa tải xong). Bài kế tiếp được nạp sẵn vào hàng của AVQueuePlayer ngay khi bài đang phát sẵn sàng → hết bài là
/// chuyển liền, kể cả khi app ở nền / khoá màn (iOS 6 không cho app ở nền *bắt đầu* phát nếu tiếng đã dừng — TubeMini D34).
///
/// - Hàng chờ là danh sách khoá bài (tới 5000 bài — «Phát hết» cả thư viện); thông tin bài lấy dần theo cửa sổ quanh bài đang
///   phát (`library.songs`).
/// - Màn hình khoá / tai nghe / Control Center của iOS: MPNowPlayingInfoCenter + sự kiện điều khiển từ xa.
/// - Cuộc gọi đến: tạm dừng, gọi xong phát tiếp; rút tai nghe: tạm dừng.
/// - «Nghe tiếp»: hàng chờ, bài, vị trí lưu 10 giây một lần; mở app không tự phát.
@interface GMPlayer : NSObject
+ (instancetype)shared;
/// Phiên âm thanh, nghe ngắt / đổi đường tiếng, khôi phục hàng chờ đã lưu (không phát).
- (void)setup;

@property (nonatomic, readonly) NSDictionary *current;
@property (nonatomic, readonly) NSArray *queueKeys;
@property (nonatomic, readonly) NSInteger index;
/// Người dùng đang muốn phát (kể cả lúc đang chờ tải).
@property (nonatomic, readonly) BOOL playing;
/// Đang chờ dữ liệu (mở bài, tua, mạng chậm).
@property (nonatomic, readonly) BOOL buffering;
@property (nonatomic, readonly) double position;
@property (nonatomic, readonly) double duration;
@property (nonatomic) BOOL shuffle;
@property (nonatomic) GMRepeatMode repeat;

/// Hẹn giờ tắt: phút (0 = tắt, -1 = hết bài đang phát).
- (void)setSleepMinutes:(NSInteger)minutes;
/// Còn bao nhiêu giây tới giờ tắt (0 = không hẹn); -1 = tắt khi hết bài.
- (NSTimeInterval)sleepRemaining;

/// Thông tin bài (nil nếu chưa lấy).
- (NSDictionary *)infoFor:(NSString *)key;
/// Lấy thông tin các bài còn thiếu rồi gọi done (hàng chờ dài).
- (void)fetchInfo:(NSArray *)keys done:(void (^)(void))done;

/// Phát danh sách bài (dict), bắt đầu ở bài `i`.
- (void)playSongs:(NSArray *)songs at:(NSInteger)i;
/// Phát theo danh sách khoá (cả thư viện…), `info` = thông tin đã có (khoá → dict). `shuffle`: trộn (bài `i` lên đầu).
- (void)playKeys:(NSArray *)keys at:(NSInteger)i info:(NSDictionary *)info shuffle:(BOOL)shuffle;
- (void)togglePlay;
- (void)play;
- (void)pause;
- (void)next;
- (void)previous;
- (void)seek:(double)seconds;
/// Nhảy tới bài thứ `i` của hàng chờ.
- (void)jumpTo:(NSInteger)i;
/// Phát tiếp theo (chen ngay sau bài đang phát) / thêm vào cuối hàng chờ.
- (void)insertNext:(NSDictionary *)song;
- (void)append:(NSDictionary *)song;
- (void)removeAt:(NSInteger)i;
- (void)moveFrom:(NSInteger)from to:(NSInteger)to;
/// Cập nhật thông tin bài trong hàng (đổi Yêu thích, đã tải…).
- (void)updateSong:(NSDictionary *)song;
/// Sự kiện điều khiển từ xa (màn hình khoá, tai nghe) — GMAppDelegate chuyển vào.
- (void)remoteEvent:(UIEvent *)event;
- (void)saveState;
- (void)didEnterBackground;
- (void)willEnterForeground;
@end
