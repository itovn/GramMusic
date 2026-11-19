#import <Foundation/Foundation.h>

/// Cập nhật OTA (M26 — chép từ GramMini D33): đọc bản mới nhất trên GitHub Releases (địa chỉ API ở Info.plist `GMUpdateURL`; bản thử đổi
/// được bằng NSUserDefaults `update.api`), tìm tệp `vn.grammusic.app_<bản>_iphoneos-arm.deb`. Kiểm lúc mở app (sau 10 giây) và
/// khi quay lại app (tối đa 6 giờ một lần), hỏi trước khi cập nhật («Để sau» = không hỏi lại bản đó trong 24 giờ); tải qua lõi
/// (`http.download` — rustls, iOS 5 / 6 không tự vào được GitHub), kiểm kích thước + SHA-256 (`digest` của GitHub), rồi gọi
/// /usr/libexec/grammusic/gmupdate (root, setuid): dpkg -i → khởi động lại SpringBoard → mở lại app.
@interface GMUpdate : NSObject
/// Gọi một lần lúc mở app.
+ (void)start;
/// Cài đặt → Phiên bản: kiểm ngay, báo cả khi đang dùng bản mới nhất.
+ (void)checkManual;
@end
