#import "GMDebug.h"
#import "GMAppDelegate.h"
#import "GMCore.h"
#import "GMLocalViewController.h"
#import "GMLog.h"
#import "GMPlayer.h"
#import "GMRootController.h"
#import "GMUpdate.h"
#import <UIKit/UIKit.h>
#include <notify.h>

static NSString *const kCmdFile = @"/var/mobile/Documents/GramMusic-cmd.txt";

@implementation GMDebug

+ (void)start {
    [NSTimer scheduledTimerWithTimeInterval:0.5 target:self selector:@selector(tick) userInfo:nil repeats:YES];
    // Bản thử: giữ màn hình sáng để SSH / chụp màn hình không bị mất khi máy tự khoá
    [UIApplication sharedApplication].idleTimerDisabled = YES;
    GMLog(@"[thử] đọc lệnh từ %@", kCmdFile);
}

+ (void)tick {
    NSString *s = [NSString stringWithContentsOfFile:kCmdFile encoding:NSUTF8StringEncoding error:NULL];
    if (!s) return;
    [[NSFileManager defaultManager] removeItemAtPath:kCmdFile error:NULL];
    for (NSString *raw in [s componentsSeparatedByString:@"\n"]) {
        NSString *line = [raw stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
        if (line.length) [self run:line];
    }
}

/// Màn trên cùng: màn đang phủ (Đang phát…) hoặc màn đầu của mục đang chọn.
+ (UIViewController *)top {
    GMRootController *root = GMRoot();
    UIViewController *vc = root.presentedViewController;
    if (!vc) {
        for (UIViewController *c in root.childViewControllers) vc = c;
    }
    if ([vc isKindOfClass:[UINavigationController class]]) vc = ((UINavigationController *)vc).topViewController;
    return vc;
}

static NSString *GMShort(id v) {
    NSData *d = v ? [NSJSONSerialization dataWithJSONObject:v options:0 error:NULL] : nil;
    NSString *s = d ? [[NSString alloc] initWithData:d encoding:NSUTF8StringEncoding] : [v description];
    return s.length > 600 ? [[s substringToIndex:600] stringByAppendingString:@"…"] : s;
}

+ (void)run:(NSString *)line {
    NSRange sp = [line rangeOfString:@" "];
    NSString *cmd = sp.location == NSNotFound ? line : [line substringToIndex:sp.location];
    NSString *arg = sp.location == NSNotFound ? @"" : [line substringFromIndex:sp.location + 1];
    GMLog(@"[thử] %@", line);
    GMPlayer *p = [GMPlayer shared];
    UIViewController *top = [self top];
    if ([cmd isEqual:@"call"]) {
        NSRange s2 = [arg rangeOfString:@" "];
        NSString *m = s2.location == NSNotFound ? arg : [arg substringToIndex:s2.location];
        NSString *js = s2.location == NSNotFound ? @"{}" : [arg substringFromIndex:s2.location + 1];
        id a = [NSJSONSerialization JSONObjectWithData:[js dataUsingEncoding:NSUTF8StringEncoding] options:0 error:NULL];
        long long t = GMNowMs();
        [GMCore call:m args:[a isKindOfClass:[NSDictionary class]] ? a : @{} done:^(id ok, NSDictionary *err) {
            GMLog(@"[thử] %@ → %lld ms %@", m, GMNowMs() - t, err ? GMShort(err) : GMShort(ok));
        }];
    } else if ([cmd isEqual:@"tab"]) {
        GMRoot().selectedIndex = arg.integerValue;
    } else if ([cmd isEqual:@"select"]) {
        NSArray *n = [arg componentsSeparatedByString:@" "];
        UITableView *tv = [top respondsToSelector:@selector(tableView)] ? [(id)top tableView] : nil;
        if (!tv || n.count < 2) {
            GMLog(@"[thử] select: không có bảng");
            return;
        }
        NSIndexPath *ip = [NSIndexPath indexPathForRow:[[n objectAtIndex:1] integerValue] inSection:[[n objectAtIndex:0] integerValue]];
        // Bảng chưa tải xong (vừa đổi mục) → không chạm (UIKit ném lỗi «invalid index path» — crash bản thử 17/11)
        if (ip.section >= tv.numberOfSections || ip.row >= [tv numberOfRowsInSection:ip.section]) {
            GMLog(@"[thử] select: bảng chưa có hàng %ld/%ld", (long)ip.section, (long)ip.row);
            return;
        }
        [tv selectRowAtIndexPath:ip animated:NO scrollPosition:UITableViewScrollPositionMiddle];
        [tv.delegate tableView:tv didSelectRowAtIndexPath:ip];
    } else if ([cmd isEqual:@"back"]) {
        [top.navigationController popViewControllerAnimated:YES];
    } else if ([cmd isEqual:@"dismiss"]) {
        [GMRoot() dismissViewControllerAnimated:YES completion:nil];
    } else if ([cmd isEqual:@"nowplaying"]) {
        [GMRoot() openNowPlaying];
    } else if ([cmd isEqual:@"toggle"]) {
        [p togglePlay];
    } else if ([cmd isEqual:@"next"]) {
        [p next];
    } else if ([cmd isEqual:@"prev"]) {
        [p previous];
    } else if ([cmd isEqual:@"seek"]) {
        [p seek:arg.doubleValue];
    } else if ([cmd isEqual:@"shuffle"]) {
        p.shuffle = !p.shuffle;
    } else if ([cmd isEqual:@"repeat"]) {
        p.repeat = (p.repeat + 1) % 3;
    } else if ([cmd isEqual:@"sleep"]) {
        [p setSleepMinutes:arg.integerValue];
    } else if ([cmd isEqual:@"playkey"]) {
        [GMCore call:@"library.songs" args:@{@"keys": [arg componentsSeparatedByString:@" "]} done:^(id ok, NSDictionary *err) {
            if ([ok isKindOfClass:[NSArray class]] && [ok count]) [p playSongs:ok at:0];
            else GMLog(@"[thử] playkey: không thấy bài");
        }];
    } else if ([cmd isEqual:@"playfile"]) {
        // Phát tệp trên máy (đường dẫn cách nhau bằng «|»)
        NSMutableArray *keys = [NSMutableArray array];
        for (NSString *path in [arg componentsSeparatedByString:@"|"]) [keys addObject:[@"file:" stringByAppendingString:path]];
        [p playKeys:keys at:0 info:@{} shuffle:NO];
    } else if ([cmd isEqual:@"openpath"]) {
        if ([top respondsToSelector:@selector(debugOpenPath:)]) [(id)top debugOpenPath:arg];
        else GMLog(@"[thử] openpath: màn %@ không mở được thư mục", NSStringFromClass([top class]));
    } else if ([cmd isEqual:@"snap"]) {
        // Ảnh giao diện của app (vẽ lại cửa sổ — không dính nút AssistiveTouch / thanh trạng thái) cho trang giới thiệu
        UIWindow *w = [UIApplication sharedApplication].keyWindow;
        UIGraphicsBeginImageContextWithOptions(w.bounds.size, YES, 0);
        [w.layer renderInContext:UIGraphicsGetCurrentContext()];
        UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
        UIGraphicsEndImageContext();
        NSString *path = arg.length ? arg : @"/var/mobile/Documents/gmsnap.png";
        [UIImagePNGRepresentation(img) writeToFile:path atomically:YES];
        GMLog(@"[thử] snap → %@ (%.0fx%.0f)", path, img.size.width * img.scale, img.size.height * img.scale);
    } else if ([cmd isEqual:@"notify"]) {
        // Báo Darwin (vd. thử tweak: vn.grammusic.lockcontrol/debug.lock | debug.dump)
        notify_post([arg UTF8String]);
    } else if ([cmd isEqual:@"perform"]) {
        // Gọi một hàm không tham số của màn trên cùng (vd. `perform toggleLyrics` trên màn Đang phát)
        SEL sel = NSSelectorFromString(arg);
        if ([top respondsToSelector:sel]) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
            [top performSelector:sel];
#pragma clang diagnostic pop
        } else {
            GMLog(@"[thử] perform: %@ không có %@", NSStringFromClass([top class]), arg);
        }
    } else if ([cmd isEqual:@"updateapi"]) {
        // updateapi <url> — nguồn bản mới để thử OTA (rỗng = dùng Info.plist); updatecheck — kiểm ngay như chạm «Phiên bản»
        if (arg.length) [[NSUserDefaults standardUserDefaults] setObject:arg forKey:@"update.api"];
        else [[NSUserDefaults standardUserDefaults] removeObjectForKey:@"update.api"];
        [[NSUserDefaults standardUserDefaults] removeObjectForKey:@"update.snooze"];
        [[NSUserDefaults standardUserDefaults] synchronize];
    } else if ([cmd isEqual:@"updatecheck"]) {
        [GMUpdate checkManual];
    } else if ([cmd isEqual:@"alertbtn"]) {
        // alertbtn <i> — bấm nút i của hộp thoại của app (UIAlertView của GramMusic, không phải hộp hệ thống)
        for (UIWindow *w in [UIApplication sharedApplication].windows) {
            for (UIView *v in w.subviews) {
                if (![v isKindOfClass:[UIAlertView class]]) continue;
                UIAlertView *a = (UIAlertView *)v;
                GMLog(@"[thử] hộp «%@»: nút %ld «%@»", a.title, (long)arg.integerValue, [a buttonTitleAtIndex:arg.integerValue]);
                if ([a.delegate respondsToSelector:@selector(alertView:clickedButtonAtIndex:)]) [a.delegate alertView:a clickedButtonAtIndex:arg.integerValue];
                [a dismissWithClickedButtonIndex:arg.integerValue animated:YES];
            }
        }
    } else if ([cmd isEqual:@"state"]) {
        NSDictionary *c = p.current;
        GMLog(@"[thử] trình phát: bài %@ (%@), %ld/%lu, phát %d, chờ %d, %.1f/%.1f s, trộn %d, lặp %ld, nền %ld",
              [c objectForKey:@"key"], [c objectForKey:@"ext"], (long)p.index + 1, (unsigned long)p.queueKeys.count, p.playing, p.buffering,
              p.position, p.duration, p.shuffle, (long)p.repeat, (long)[UIApplication sharedApplication].applicationState);
    } else {
        GMLog(@"[thử] lệnh lạ: %@", cmd);
    }
}

@end
