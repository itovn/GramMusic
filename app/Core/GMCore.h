#import <Foundation/Foundation.h>

/// Lõi Rust (core/crates/gm-ffi → libgm_ffi.a). Lệnh dạng «nhóm.lệnh» + tham số JSON — xem docs/FFI.md.
@interface GMCore : NSObject
/// Mở lõi một lần lúc mở app (thư mục dữ liệu GMDataDir()).
+ (BOOL)start;
/// Gọi lệnh chạy nền; done(ok, err) trên luồng chính. err: @{code, msg, …} hoặc nil.
+ (void)call:(NSString *)method args:(NSDictionary *)args done:(void (^)(id ok, NSDictionary *err))done;
@end

/// Sự kiện lõi đẩy lên (tin mới, sửa, xoá, trạng thái đăng nhập): userInfo = @{@"e": tên, …}. Phát trên luồng chính.
extern NSString *const GMCoreEventNotification;

/// Câu báo lỗi để hiện cho người dùng.
NSString *GMErrorText(NSDictionary *err);
