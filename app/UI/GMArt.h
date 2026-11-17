#import <UIKit/UIKit.h>

/// Hình vẽ cho trình phát nhạc (CoreGraphics lúc chạy, cả iOS 5 / 6 — không kèm ảnh): nút phát / dừng / bài trước / sau, trộn,
/// lặp, tim, hàng chờ, tải về…; ảnh bìa tạm (khi bài không có ảnh bìa) màu theo bài; nút trượt của thanh thời gian.
@interface GMArt : NSObject
/// Biểu tượng `name` vuông cạnh `size` điểm, màu `color`. Tên: play, pause, next, prev, shuffle, repeat, repeat1, heart,
/// heartFill, queue, download, downloaded, more, timer, plus, close, note, search.
+ (UIImage *)glyph:(NSString *)name size:(CGFloat)size color:(UIColor *)color;
/// Ảnh bìa tạm: ô vuông bo góc, dải màu theo khoá bài, nốt nhạc trắng ở giữa.
+ (UIImage *)placeholderFor:(NSString *)key size:(CGFloat)size;
/// Nút trượt tròn bóng cho UISlider (thanh thời gian / âm lượng).
+ (UIImage *)sliderThumb;
/// Rãnh thanh trượt: phần đã qua (màu chủ đạo) / phần còn lại (mờ).
+ (UIImage *)sliderTrack:(BOOL)played;
/// Nhãn nhỏ bo góc («FLAC», «320»…) dùng trong ô bài hát.
+ (UIImage *)badge:(NSString *)text color:(UIColor *)color;
@end

/// «3:05», «1:02:03».
NSString *GMDuration(double seconds);
