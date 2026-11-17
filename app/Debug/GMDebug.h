#import <Foundation/Foundation.h>

/// Chỉ có trong bản thử (DEBUG, như GramMini): đọc lệnh từ /var/mobile/Documents/GramMusic-cmd.txt (mỗi dòng một lệnh, đọc xong
/// xoá tệp) để thử app qua SSH — kèm `cbscreen` chụp màn hình. Kết quả ghi vào app.log («[thử] …»). Lệnh:
///   call <lệnh> [json]   gọi lệnh lõi (docs/FFI.md), ghi kết quả (rút gọn)
///   tab <0–4>            chọn mục thanh dưới (Thư viện, Nguồn, Playlist, Đã tải, Cài đặt)
///   select <mục> <hàng>  chạm hàng của bảng đang hiện
///   back | dismiss       quay lại màn trước / đóng màn đang phủ
///   nowplaying           mở màn Đang phát
///   toggle | next | prev | seek <giây> | shuffle | repeat | sleep <phút>
///   state                ghi trạng thái trình phát
@interface GMDebug : NSObject
+ (void)start;
@end
