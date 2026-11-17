#import <UIKit/UIKit.h>

/// Nhạc trên máy (DECISIONS M15):
/// - **Thư viện app Nhạc** (bài chép từ iTunes) — MediaPlayer, khoá `ipod:<persistentID>`, app tự phát qua `assetURL`
///   (`ipod-library://…`); bài có DRM / chưa tải từ iCloud không có assetURL → không phát được.
/// - **Tệp trên máy** — khoá `file:<đường dẫn>`, lõi lo (local.rs): thông tin tệp, URL phát (FLAC / OGG / Opus lõi giải mã).
@interface GMLocalMusic : NSObject

+ (BOOL)isLocalKey:(NSString *)key;
+ (BOOL)isIPodKey:(NSString *)key;

/// Mọi bài của app Nhạc (dict như Song của lõi + `album`, `local`), xếp theo tên. Đọc ở luồng nền, `done` ở luồng chính.
+ (void)loadIPodSongs:(void (^)(NSArray *songs))done;
/// Một bài theo khoá (nil: không còn trên máy). Luồng chính.
+ (NSDictionary *)songForKey:(NSString *)key;
/// URL cho AVPlayer (nil: DRM / không có tệp).
+ (NSURL *)urlForKey:(NSString *)key;
/// Ảnh bìa (nil: không có).
+ (UIImage *)artworkForKey:(NSString *)key pixels:(CGFloat)px;

/// Đuôi tệp phát được (AVPlayer đọc thẳng hoặc lõi giải mã) — trình duyệt tệp chỉ hiện các tệp này.
+ (BOOL)playableFile:(NSString *)name;

@end
