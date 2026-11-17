#import <UIKit/UIKit.h>

/// Ảnh: tải qua lõi (media.cover / media.avatar → tệp đệm trên đĩa), giải mã + thu nhỏ đúng cỡ hiển thị bằng ImageIO ở hàng
/// nền ưu tiên thấp (A4 một nhân: giải mã nhiều ảnh cùng lúc làm treo luồng chính — bài học Côn Bằng TV), bộ đệm RAM nhỏ
/// (8 MB khi máy < 300 MB RAM). done() luôn trên luồng chính; ảnh đã có trong bộ đệm thì gọi ngay. Chép từ GramMini.
@interface GMImages : NSObject
/// Ảnh đại diện tròn của nguồn (cỡ `size` điểm). Không có ảnh (photo nil) hoặc tải lỗi thì không gọi done.
+ (void)avatarForPeer:(NSString *)peer photo:(NSString *)photo dc:(NSNumber *)dc size:(CGFloat)size done:(void (^)(UIImage *img))done;
/// Ảnh bìa của bài (`song`: dict có key, thumb) cỡ `size` điểm. Bài không có ảnh bìa / tải lỗi: không gọi done.
+ (void)cover:(NSDictionary *)song size:(CGFloat)size done:(void (^)(UIImage *img))done;
+ (UIImage *)cachedCover:(NSString *)songKey size:(CGFloat)size;
/// Thiếu bộ nhớ: xoá bộ đệm RAM.
+ (void)purge;
@end
