#import "GMImages.h"
#import "GMCore.h"
#import "GMLocalMusic.h"
#import "GMLog.h"
#import <ImageIO/ImageIO.h>

static NSCache *gCache;
static dispatch_queue_t gQueue;
static NSMutableDictionary *gWaiting;  // khoá → [done…] (gộp yêu cầu trùng)
static NSMutableDictionary *gFailed;   // khoá → lúc lấy lỗi: 30 giây sau mới hỏi lại (danh sách vẽ lại mỗi giây — mất mạng thì khỏi
                                       // hỏi lõi liên tục, đo 17/11: ~3 lần / giây)

@implementation GMImages

+ (void)initialize {
    if (self != [GMImages class]) return;
    gCache = [[NSCache alloc] init];
    BOOL lowRam = [NSProcessInfo processInfo].physicalMemory < 300ull * 1024 * 1024;
    gCache.totalCostLimit = (lowRam ? 8 : 24) * 1024 * 1024;
    gQueue = dispatch_queue_create("gm.images", DISPATCH_QUEUE_SERIAL);
    dispatch_set_target_queue(gQueue, dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_LOW, 0));
    gWaiting = [NSMutableDictionary dictionary];
    gFailed = [NSMutableDictionary dictionary];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(purge)
                                                 name:UIApplicationDidReceiveMemoryWarningNotification object:nil];
}

+ (void)purge {
    [gCache removeAllObjects];
}

/// Giải mã + thu nhỏ (ImageIO), tuỳ chọn cắt tròn. Chạy ở hàng nền.
static UIImage *GMDecode(NSString *path, CGFloat maxPixel, BOOL round, BOOL alpha) {
    NSURL *url = [NSURL fileURLWithPath:path];
    CGImageSourceRef src = CGImageSourceCreateWithURL((__bridge CFURLRef)url, NULL);
    if (!src) return nil;
    NSDictionary *opt = @{
        (__bridge id)kCGImageSourceCreateThumbnailFromImageAlways: @YES,
        (__bridge id)kCGImageSourceCreateThumbnailWithTransform: @YES,
        (__bridge id)kCGImageSourceThumbnailMaxPixelSize: @((int)maxPixel),
    };
    CGImageRef cg = CGImageSourceCreateThumbnailAtIndex(src, 0, (__bridge CFDictionaryRef)opt);
    CFRelease(src);
    if (!cg) return nil;
    CGFloat scale = [UIScreen mainScreen].scale;
    UIImage *img = [UIImage imageWithCGImage:cg scale:scale orientation:UIImageOrientationUp];
    CGImageRelease(cg);
    if (round) {
        CGFloat s = MIN(img.size.width, img.size.height);
        UIGraphicsBeginImageContextWithOptions(CGSizeMake(s, s), NO, scale);
        [[UIBezierPath bezierPathWithOvalInRect:CGRectMake(0, 0, s, s)] addClip];
        [img drawInRect:CGRectMake((s - img.size.width) / 2, (s - img.size.height) / 2, img.size.width, img.size.height)];
        img = UIGraphicsGetImageFromCurrentImageContext();
        UIGraphicsEndImageContext();
    } else {
        // Vẽ sẵn ra bitmap (khỏi giải mã lần nữa trên luồng chính khi hiện)
        UIGraphicsBeginImageContextWithOptions(img.size, !alpha, scale);
        [img drawAtPoint:CGPointZero];
        img = UIGraphicsGetImageFromCurrentImageContext();
        UIGraphicsEndImageContext();
    }
    return img;
}

static NSUInteger GMCost(UIImage *img) {
    return (NSUInteger)(img.size.width * img.size.height * img.scale * img.scale * 4);
}

/// Tải tệp (nếu chưa có) rồi giải mã; gộp các yêu cầu cùng khoá.
+ (void)key:(NSString *)key file:(NSString *)file fetch:(void (^)(void (^got)(NSString *path)))fetch maxPixel:(CGFloat)px
      round:(BOOL)round done:(void (^)(UIImage *))done {
    [self key:key file:file fetch:fetch maxPixel:px round:round alpha:NO done:done];
}

+ (void)key:(NSString *)key file:(NSString *)file fetch:(void (^)(void (^got)(NSString *path)))fetch maxPixel:(CGFloat)px
      round:(BOOL)round alpha:(BOOL)alpha done:(void (^)(UIImage *))done {
    UIImage *hit = [gCache objectForKey:key];
    if (hit) {
        done(hit);
        return;
    }
    NSDate *failed = [gFailed objectForKey:key];
    if (failed && -[failed timeIntervalSinceNow] < 30) return;
    NSMutableArray *waiting = [gWaiting objectForKey:key];
    if (waiting) {
        [waiting addObject:[done copy]];
        return;
    }
    [gWaiting setObject:[NSMutableArray arrayWithObject:[done copy]] forKey:key];
    void (^decode)(NSString *) = ^(NSString *path) {
        if (!path) {
            [gWaiting removeObjectForKey:key];
            [gFailed setObject:[NSDate date] forKey:key];
            return;
        }
        dispatch_async(gQueue, ^{
            UIImage *img = GMDecode(path, px, round, alpha);
            dispatch_async(dispatch_get_main_queue(), ^{
                NSArray *cbs = [gWaiting objectForKey:key];
                [gWaiting removeObjectForKey:key];
                if (!img) {
                    [gFailed setObject:[NSDate date] forKey:key];
                    return;
                }
                [gFailed removeObjectForKey:key];
                [gCache setObject:img forKey:key cost:GMCost(img)];
                for (void (^cb)(UIImage *) in cbs) cb(img);
            });
        });
    };
    if (file && [[NSFileManager defaultManager] fileExistsAtPath:file]) {
        GMTouchFile(file);
        decode(file);
    }
    else fetch(decode);
}

+ (void)avatarForPeer:(NSString *)peer photo:(NSString *)photo dc:(NSNumber *)dc size:(CGFloat)size done:(void (^)(UIImage *))done {
    if (!peer || !photo) return;
    NSString *key = [NSString stringWithFormat:@"a|%@|%@|%.0f", peer, photo, size];
    NSString *file = [GMDataDir() stringByAppendingFormat:@"/avatars/%@_%@.jpg", peer, photo];
    [self key:key file:file
        fetch:^(void (^got)(NSString *)) {
            [GMCore call:@"media.avatar" args:@{@"peer": peer, @"photo": photo, @"dc": dc ?: @0} done:^(id ok, NSDictionary *err) {
                got(err ? nil : [ok objectForKey:@"path"]);
            }];
        }
        maxPixel:size * [UIScreen mainScreen].scale round:YES done:done];
}

+ (NSString *)coverKey:(NSString *)songKey px:(CGFloat)px {
    return [NSString stringWithFormat:@"c|%@|%.0f", songKey, px];
}

+ (UIImage *)cachedCover:(NSString *)songKey size:(CGFloat)size {
    return [gCache objectForKey:[self coverKey:songKey px:size * [UIScreen mainScreen].scale]];
}

+ (void)cover:(NSDictionary *)song size:(CGFloat)size done:(void (^)(UIImage *))done {
    NSString *k = [song objectForKey:@"key"];
    if (![k isKindOfClass:[NSString class]] || ![[song objectForKey:@"thumb"] boolValue]) return;
    CGFloat px = size * [UIScreen mainScreen].scale;
    if ([GMLocalMusic isIPodKey:k]) {
        // Bài app Nhạc: ảnh bìa trong thư viện iPod (MediaPlayer), thu nhỏ ở hàng nền
        NSString *ck = [self coverKey:k px:px];
        UIImage *hit = [gCache objectForKey:ck];
        if (hit) {
            done(hit);
            return;
        }
        dispatch_async(gQueue, ^{
            UIImage *img = [GMLocalMusic artworkForKey:k pixels:px];
            dispatch_async(dispatch_get_main_queue(), ^{
                if (!img) return;
                [gCache setObject:img forKey:ck cost:GMCost(img)];
                done(img);
            });
        });
        return;
    }
    if ([k hasPrefix:@"file:"]) return;  // tệp trên máy: chưa đọc ảnh nhúng trong tệp
    NSString *file = [GMDataDir() stringByAppendingFormat:@"/covers/%@.jpg", k];
    [self key:[self coverKey:k px:px] file:file
        fetch:^(void (^got)(NSString *)) {
            [GMCore call:@"media.cover" args:@{@"key": k} done:^(id ok, NSDictionary *err) {
                got(err ? nil : [ok objectForKey:@"path"]);
            }];
        }
        maxPixel:px round:NO done:done];
}

@end
