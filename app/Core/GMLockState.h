#import <Foundation/Foundation.h>

/// Cầu nối với tweak GramLockControl (màn hình khoá — DECISIONS M20): ghi trạng thái phát (bài, vị trí lúc ghi, đang phát?, lời chạy) vào
/// ~/Library/GramMusic/lockstate.plist mỗi khi có thay đổi rồi báo «vn.grammusic.lockcontrol/state»; nhận lệnh «…/cmd.toggle | next |
/// prev | seek» (tua: số mili giây trong notify_set_state). Chỉ ghi khi tweak đã cài.
@interface GMLockState : NSObject
+ (void)start;
/// Tweak đã cài + đang bật + hiện thanh tiến trình (đọc cài đặt của tweak, nhớ 5 giây) — khi đó app khỏi ghi thời gian lên màn hình khoá.
+ (BOOL)tweakShowsProgress;
@end
