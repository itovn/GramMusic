#import <UIKit/UIKit.h>

/// Giao diện iOS 6 + kính trong (D23): bố cục Telegram 4.9; thanh trên bóng kiểu iOS 6 (nút viền nổi, nút quay lại hình mũi tên),
/// thanh dưới kính khói, bảng / bong bóng kính mờ trên hình nền loang màu. Mọi ảnh tự vẽ bằng CoreGraphics lúc chạy — chạy cả
/// iOS 5 / 6 (không có làm mờ thời gian thực: «kính» = lớp bán trong suốt trên hình nền tĩnh).
typedef NS_ENUM(NSInteger, GMThemeMode) { GMThemeLight, GMThemeDark, GMThemeAuto };

@interface GMTheme : NSObject
/// Áp giao diện theo chế độ (Sáng / Tối / Tự động theo giờ: tối từ 19:00 tới 6:00). Đổi chế độ → dựng lại giao diện
/// (GMAppDelegate reloadTheme) — màu / ảnh mới chỉ vào view tạo sau.
+ (void)apply;
+ (NSInteger)mode;
+ (void)setMode:(NSInteger)mode;
+ (NSString *)modeText:(NSInteger)mode;
/// Chế độ tối đang áp / nên áp (theo cài đặt và giờ).
+ (BOOL)dark;
+ (BOOL)wantsDark;
+ (UIColor *)text;
+ (UIColor *)cellBackground;
+ (UIColor *)barBackground;
+ (UIColor *)fieldBackground;
+ (UIColor *)placeholderFill;
+ (UIColor *)pillBackground;
+ (UIColor *)selectedCell;
+ (UIKeyboardAppearance)keyboard;
/// Nền kính của ô bảng (thường / nhóm), vạch kẻ trên kính.
+ (UIColor *)glassFill;
+ (UIColor *)glassGrouped;
+ (UIColor *)glassLine;
/// Chữ nút thanh trên; tiêu đề thanh trên + bóng chữ.
+ (UIColor *)barText;
+ (UIColor *)navTitle;
+ (UIColor *)navTitleShadow;
+ (CGSize)navShadowOffset;
/// Hình nền chung (vuông, cạnh = cạnh dài của màn — phủ cả dọc lẫn ngang).
+ (UIImage *)wallpaper;
/// Nền kính của thanh nhập tin / khung trả lời (co giãn theo chiều cao).
+ (UIImage *)glassBar;
/// Ô nhập bo tròn lõm (co giãn).
+ (UIImage *)fieldImage;
/// Nút «Gửi» xanh bóng (ảnh cao 44, viên giữa cao 32 — co giãn theo bề ngang).
+ (UIImage *)sendButtonImage:(BOOL)pressed;
/// Nền một hàng của thẻ kính (danh sách chat): hàng đầu / cuối bo góc; lề hai bên 8 pt nằm sẵn trong ảnh.
+ (UIImage *)cardRow:(BOOL)first last:(BOOL)last selected:(BOOL)selected;
/// Nền số chưa đọc: viên bóng xanh (tắt tiếng: xám) viền trắng, cao 22.
+ (UIImage *)badgeImage:(BOOL)muted;
/// Mục thanh dưới: «contacts», «chats», «settings» (ảnh tự vẽ, chọn / không chọn — không để iOS tô màu).
+ (UITabBarItem *)tabItem:(NSString *)title icon:(NSString *)icon;
/// Nút chữ hình mũi tên trỏ trái (như nút quay lại của iOS 6) — cho bước lùi không phải màn trước trong thanh điều hướng.
+ (UIBarButtonItem *)backItem:(NSString *)title target:(id)target action:(SEL)action;
/// Nút có biểu tượng trên thanh trên, nền viền nổi như nút chữ.
+ (UIBarButtonItem *)barButtonIcon:(NSString *)icon target:(id)target action:(SEL)action;
/// Như trên với ảnh cho sẵn (biểu tượng GMArt).
+ (UIBarButtonItem *)barButtonImage:(UIImage *)image target:(id)target action:(SEL)action;
/// Biểu tượng nét mảnh theo màu cho trước.
+ (UIImage *)icon:(NSString *)name color:(UIColor *)color;
+ (UIColor *)accent;
+ (UIColor *)background;
+ (UIColor *)chatBackground;
+ (UIColor *)secondaryText;
+ (UIColor *)separator;
+ (UIColor *)checkColor;
+ (UIColor *)mutedBadge;
/// Màu theo peer (ảnh đại diện chữ cái, tên người gửi trong nhóm).
+ (UIColor *)colorFor:(NSString *)peer;
/// Ảnh đại diện tạm: chữ cái đầu trên nền tròn màu (có bộ đệm).
+ (UIImage *)avatarFor:(NSString *)peer name:(NSString *)name size:(CGFloat)size;
/// Biểu tượng chủ đề diễn đàn: chữ cái đầu của tên trên nền tròn màu `rgb` (icon_color của Telegram; 0 = xám). Biểu tượng
/// emoji riêng (icon_emoji_id) chưa vẽ — dùng chữ cái.
+ (UIImage *)topicIcon:(NSString *)title color:(NSInteger)rgb size:(CGFloat)size;
/// Cắt ảnh thành hình tròn cỡ `size` pt (ảnh đại diện vừa đổi).
+ (UIImage *)roundImage:(UIImage *)img size:(CGFloat)size;
/// Nền bong bóng co giãn được: tin ra / vào, có đuôi hay không (tin cuối của một cụm mới có đuôi).
+ (UIImage *)bubbleOut:(BOOL)outgoing tail:(BOOL)tail;
/// Đuôi bong bóng rộng ngần này (phần nội dung lệch vào).
+ (CGFloat)tailWidth;
/// Giờ ngắn cho danh sách: hôm nay «14:05», trong tuần «T2», năm nay «16/11», khác năm «16/11/25».
+ (NSString *)shortTime:(NSNumber *)unix;
+ (NSString *)clock:(NSNumber *)unix;
/// Ngày cho vạch chia trong chat: «Hôm nay», «Hôm qua», «16 tháng 11», «16 tháng 11, 2025».
+ (NSString *)dayTitle:(NSNumber *)unix;
/// Hai thời điểm cùng ngày (giờ máy).
+ (BOOL)sameDay:(NSNumber *)a as:(NSNumber *)b;
/// Nội dung xem trước của tin (chữ, hoặc «Ảnh, chú thích»…).
+ (NSString *)preview:(NSDictionary *)msg;
/// Nút chữ có nền màu.
+ (UIButton *)button:(NSString *)title;
/// Thông báo nhỏ hiện giữa dưới màn rồi tự ẩn.
+ (void)toast:(NSString *)text in:(UIView *)view;
/// Biểu tượng ô vuông màu cho Cài đặt: «saved» (dấu trang), «proxy» (khiên), «info» (chữ i).
+ (UIImage *)settingsIcon:(NSString *)kind;
/// Biểu tượng nét mảnh màu chủ đạo: «mic» (ghi âm), «clip» (đính kèm).
+ (UIImage *)icon:(NSString *)name;
/// Cỡ tệp dễ đọc, dấu phẩy thập phân: «812 KB», «1,5 MB», «2,25 GB».
+ (NSString *)sizeText:(double)bytes;
/// Thời gian tin tự xoá (giây): «Tắt», «1 ngày», «1 tuần», «1 tháng»; mức khác (đặt từ app khác) → «3 ngày», «5 giờ»…
+ (NSString *)ttlText:(NSInteger)seconds;
/// Các mức tự xoá cho chọn (giây, NSNumber): tắt, 1 ngày, 1 tuần, 1 tháng — như app Telegram gốc.
+ (NSArray *)ttlChoices;
/// Dòng phụ dưới tên chat từ `chat.status` / `user.status`: «trực tuyến», «truy cập lúc 14:05», «12 thành viên, 3 trực tuyến»…
+ (NSString *)statusText:(NSDictionary *)st;
/// «đang gõ…», «Lan đang ghi âm…» (nhóm: kèm tên người).
+ (NSString *)typingText:(NSString *)action name:(NSString *)name;
/// Biểu tượng 36 pt cho trình chọn tệp: thư mục; tệp (ô màu theo loại + đuôi tệp).
+ (UIImage *)folderIcon;
+ (UIImage *)fileIcon:(NSString *)ext;
@end

/// Hình nền đặt sau nội dung: ảnh nền chung canh theo gốc của màn (thanh trên kính để lộ đúng phần nền phía sau → liền mạch).
/// `striped`: phủ thêm sọc dọc kiểu bảng nhóm iOS 6 (Cài đặt…). Bảng của app tự nhận (backgroundView), màn khác tự thêm.
@interface GMWallpaperView : UIView
@property (nonatomic) BOOL striped;
@end

/// Thẻ đánh dấu: bảng không nhận hình nền riêng (màn chat — nền nằm ở view của màn); ô tự lo nền (không phủ kính chung).
#define GMTagNoWallpaper 0x474E
#define GMTagOwnBackground 0x474F

/// NSTextAlignment / NSLineBreakMode (SDK ghi «iOS 6») trùng giá trị với UITextAlignment / UILineBreakMode của iOS 5
/// trên iOS → dùng số (khỏi lỗi -Wunguarded-availability và -Wenum-conversion).
#define GMAlignCenter 1
#define GMAlignRight 2
#define GMWordWrap 0
#define GMTailTruncation 4
#define GMMiddleTruncation 5

/// Giá trị trong dict, NSNull → nil.
id GMVal(NSDictionary *d, NSString *key);
