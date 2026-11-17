#import <UIKit/UIKit.h>

/// Lời bài hát trên màn Đang phát (đè lên ảnh bìa): lời chạy (LRCLIB) tô dòng đang hát + tự cuộn, chạm dòng để tua; lời thường thì
/// cuộn tay. Lấy lời qua lõi (`lyrics.get`).
@interface GMLyricsView : UIView

/// Bài đang phát (dict Song) — đổi bài thì tự tải lời mới.
- (void)showSong:(NSDictionary *)song;
/// Vị trí phát (giây) — tô / cuộn tới dòng đang hát.
- (void)setTime:(double)t;
/// Gọi lại khi vừa bật lời bài hát trong Cài đặt.
- (void)reload;

@end
