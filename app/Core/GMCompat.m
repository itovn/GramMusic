// Chép từ Côn Bằng TV Legacy (app/Core/CBCompat.m), bỏ phần AVPlayerItem.
// Bù phần iOS 6 mà app dùng khi chạy trên iOS 5. Thêm lúc nạp app (+load, trước main), chỉ khi iOS thiếu — trên iOS 6 giữ
// nguyên hàm gốc của hệ thống.
//   • a[i] / d[k] (Objective-C subscripting): clang gọi objectAtIndexedSubscript: / objectForKeyedSubscript:… — có từ iOS 6;
//     Xcode thêm libarclite để bù khi dựng cho iOS 5, bộ dựng Theos trên Linux không có → tự bù ở đây.
//   • NSIndexPath item / indexPathForItem:inSection: (UIKit iOS 6) = row / indexPathForRow:inSection:.
//   • UILabel minimumScaleFactor (iOS 6) → minimumFontSize của iOS 5.
//   • Xoay màn hình: iOS 5 hỏi shouldAutorotateToInterfaceOrientation:, iOS 6 hỏi supportedInterfaceOrientations — màn nào
//     khai báo supportedInterfaceOrientations thì iOS 5 cũng theo đúng mặt nạ đó. Riêng màn trình bày đè lên (modal: trình phát, «Ai đang
//     xem?», miễn trừ) iOS 5 không theo hàm thay chung này — các màn đó tự khai báo shouldAutorotateToInterfaceOrientation:
//     (0.5.0 trên iPad 1: trình phát bị kéo về dọc); thanh điều hướng cũng tự khai báo (CBNavController).
//   • Thiếu bộ nhớ: iOS 5 gỡ view của các màn đang khuất (viewDidUnload) rồi dựng lại khi quay về — các màn của app dựng view
//     một lần trong viewDidLoad (đăng ký thông báo, tải dữ liệu…), dựng lại sẽ tải lại / đăng ký hai lần. iOS 6 không gỡ view
//     nữa → trên iOS 5 cũng giữ view cho các màn của app (ảnh bìa vẫn tự xoá khỏi bộ đệm khi thiếu bộ nhớ).
#import "GMCompat.h"
#import <UIKit/UIKit.h>
#import <objc/runtime.h>

BOOL GMOldIOS(void) {
    return NSFoundationVersionNumber < NSFoundationVersionNumber_iOS_6_0;
}

#pragma mark - Hàm thay

static id gm_arrayAt(NSArray *self, SEL _cmd, NSUInteger i) {
    return [self objectAtIndex:i];
}

static void gm_marraySet(NSMutableArray *self, SEL _cmd, id obj, NSUInteger i) {
    if (i == self.count) [self addObject:obj];
    else [self replaceObjectAtIndex:i withObject:obj];
}

static id gm_dictGet(NSDictionary *self, SEL _cmd, id key) {
    return [self objectForKey:key];
}

static void gm_mdictSet(NSMutableDictionary *self, SEL _cmd, id obj, id key) {
    if (obj) [self setObject:obj forKey:key];
    else [self removeObjectForKey:key];
}

static id gm_osetAt(NSOrderedSet *self, SEL _cmd, NSUInteger i) {
    return [self objectAtIndex:i];
}

static void gm_mosetSet(NSMutableOrderedSet *self, SEL _cmd, id obj, NSUInteger i) {
    if (i == self.count) [self addObject:obj];
    else [self replaceObjectAtIndex:i withObject:obj];
}

static NSInteger gm_ipItem(NSIndexPath *self, SEL _cmd) {
    return (NSInteger)[self indexAtPosition:1];
}

static NSIndexPath *gm_ipForItem(Class self, SEL _cmd, NSInteger item, NSInteger section) {
    return [NSIndexPath indexPathForRow:item inSection:section];
}

static const void *kGMScaleKey = &kGMScaleKey;

static void gm_setMinScale(UILabel *self, SEL _cmd, CGFloat f) {
    objc_setAssociatedObject(self, kGMScaleKey, @(f), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    self.minimumFontSize = MAX(6, floor(self.font.pointSize * f));
}

static CGFloat gm_minScale(UILabel *self, SEL _cmd) {
    return [objc_getAssociatedObject(self, kGMScaleKey) floatValue];
}

static IMP gm_origAutorotate;

static BOOL gm_autorotate(UIViewController *self, SEL _cmd, UIInterfaceOrientation o) {
    if ([self respondsToSelector:@selector(supportedInterfaceOrientations)]) {
        GM_IOS6_BEGIN
        NSUInteger mask = [(id)self supportedInterfaceOrientations];
        GM_IOS6_END
        return (mask & (1u << o)) != 0;
    }
    // Màn không khai báo: iPad mọi chiều, iPhone dọc (như iOS 6 với thanh điều hướng của app)
    if (UI_USER_INTERFACE_IDIOM() == UIUserInterfaceIdiomPad) return YES;
    return ((BOOL (*)(id, SEL, UIInterfaceOrientation))gm_origAutorotate)(self, _cmd, o);
}

static IMP gm_origMemoryWarning;

static void gm_memoryWarning(UIViewController *self, SEL _cmd) {
    if ([NSStringFromClass([self class]) hasPrefix:@"GM"]) return;  // màn của app: giữ view như iOS 6
    ((void (*)(id, SEL))gm_origMemoryWarning)(self, _cmd);
}

#pragma mark - Gắn

/// Thêm hàm cho lớp nếu lớp (kể cả lớp cha) chưa có. Chuỗi kiểu viết sẵn cho armv7 (NSUInteger = I, NSInteger = i,
/// CGFloat = f, BOOL = c) — app chỉ dựng armv7.
static void gm_add(Class cls, SEL sel, IMP imp, const char *types) {
    if (!cls || [cls instancesRespondToSelector:sel]) return;
    class_addMethod(cls, sel, imp, types);
}

@interface GMCompat : NSObject
@end

@implementation GMCompat

+ (void)load {
    if (!GMOldIOS()) return;
    @autoreleasepool {
        gm_add([NSArray class], @selector(objectAtIndexedSubscript:), (IMP)gm_arrayAt, "@@:I");
        gm_add([NSMutableArray class], @selector(setObject:atIndexedSubscript:), (IMP)gm_marraySet, "v@:@I");
        gm_add([NSDictionary class], @selector(objectForKeyedSubscript:), (IMP)gm_dictGet, "@@:@");
        gm_add([NSMutableDictionary class], @selector(setObject:forKeyedSubscript:), (IMP)gm_mdictSet, "v@:@@");
        gm_add([NSOrderedSet class], @selector(objectAtIndexedSubscript:), (IMP)gm_osetAt, "@@:I");
        gm_add([NSMutableOrderedSet class], @selector(setObject:atIndexedSubscript:), (IMP)gm_mosetSet, "v@:@I");

        gm_add([NSIndexPath class], @selector(item), (IMP)gm_ipItem, "i@:");
        gm_add(object_getClass([NSIndexPath class]), @selector(indexPathForItem:inSection:), (IMP)gm_ipForItem, "@#:ii");

        gm_add([UILabel class], @selector(setMinimumScaleFactor:), (IMP)gm_setMinScale, "v@:f");
        gm_add([UILabel class], @selector(minimumScaleFactor), (IMP)gm_minScale, "f@:");


        Method rot = class_getInstanceMethod([UIViewController class], @selector(shouldAutorotateToInterfaceOrientation:));
        if (rot) gm_origAutorotate = method_setImplementation(rot, (IMP)gm_autorotate);
        Method mem = class_getInstanceMethod([UIViewController class], @selector(didReceiveMemoryWarning));
        if (mem) gm_origMemoryWarning = method_setImplementation(mem, (IMP)gm_memoryWarning);
    }
}

@end
