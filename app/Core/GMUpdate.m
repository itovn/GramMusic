#import "GMUpdate.h"
#import "GMCore.h"
#import "GMLog.h"
#import "GMTheme.h"
#import "GMAlert.h"
#import <UIKit/UIKit.h>
#include <spawn.h>
#include <sys/stat.h>
#include <sys/wait.h>

extern char **environ;

static NSString *const kHelper = @"/usr/libexec/grammusic/gmupdate";
// Tệp phát hành chỉ tải từ đúng repo (GitHub chuyển tiếp sang máy chủ tệp của nó — lõi chỉ theo link https)
static NSString *const kAssetPrefix = @"https://github.com/itovn/GramMusic/releases/download/";
static const NSTimeInterval kCheckEvery = 6 * 3600;
static const NSTimeInterval kSnooze = 24 * 3600;

static NSDate *gLastCheck;
static BOOL gBusy;       // đang tải / cài
static BOOL gChecking;   // đang hỏi GitHub
static BOOL gPrompting;  // đang hiện hộp «Có bản mới»

static UIView *GMTopView(void) {
    UIViewController *vc = [UIApplication sharedApplication].keyWindow.rootViewController;
    while (vc.presentedViewController) vc = vc.presentedViewController;
    return vc.view;
}

@implementation GMUpdate

+ (NSString *)currentVersion {
    return [[NSBundle mainBundle] objectForInfoDictionaryKey:@"CFBundleShortVersionString"] ?: @"0";
}

+ (NSString *)apiURL {
#ifdef DEBUG
    // Thử OTA (lệnh thử «updateapi»): nguồn khác — bản phát hành không đổi được nguồn
    NSString *o = [[NSUserDefaults standardUserDefaults] stringForKey:@"update.api"];
    if (o.length) return o;
#endif
    return [[NSBundle mainBundle] objectForInfoDictionaryKey:@"GMUpdateURL"];
}

/// "v1.0.2" / "1.0.2" → so từng số: a mới hơn b?
+ (BOOL)version:(NSString *)a newerThan:(NSString *)b {
    NSArray *x = [[a stringByReplacingOccurrencesOfString:@"v" withString:@""] componentsSeparatedByString:@"."];
    NSArray *y = [[b stringByReplacingOccurrencesOfString:@"v" withString:@""] componentsSeparatedByString:@"."];
    for (NSUInteger i = 0; i < MAX(x.count, y.count); i++) {
        NSInteger p = i < x.count ? [[x objectAtIndex:i] integerValue] : 0, q = i < y.count ? [[y objectAtIndex:i] integerValue] : 0;
        if (p != q) return p > q;
    }
    return NO;
}

+ (void)start {
    // Tệp tải dở / đã cài của lượt trước (gmupdate cài bản chép trong thư mục của root, không xoá tệp ở đây)
    [[NSFileManager defaultManager] removeItemAtPath:[self updateDir] error:NULL];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(10 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [self check:NO];
    });
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(foreground)
                                                 name:UIApplicationWillEnterForegroundNotification object:nil];
}

+ (void)foreground {
    if (!gLastCheck || -[gLastCheck timeIntervalSinceNow] > kCheckEvery) [self check:NO];
}

+ (void)checkManual {
    [GMTheme toast:@"Đang kiểm tra bản mới…" in:GMTopView()];
    [self check:YES];
}

+ (void)check:(BOOL)manual {
    NSString *api = [self apiURL];
    if (!api.length) {
        if (manual) [GMTheme toast:@"Bản này chưa đặt nguồn cập nhật." in:GMTopView()];
        return;
    }
    if (gBusy || gChecking || gPrompting) {
        if (manual) [GMTheme toast:gBusy ? @"Đang cập nhật…" : @"Đang kiểm tra bản mới…" in:GMTopView()];
        return;
    }
    gChecking = YES;
    gLastCheck = [NSDate date];
    long long t0 = GMNowMs();
    [GMCore call:@"http.get" args:@{@"url": api, @"headers": @{@"Accept": @"application/vnd.github+json"}}
            done:^(id ok, NSDictionary *err) {
                gChecking = NO;
                NSDictionary *rel = nil;
                if ([GMVal(ok, @"code") intValue] == 200) {
                    id j = [NSJSONSerialization JSONObjectWithData:[GMVal(ok, @"text") ?: @"" dataUsingEncoding:NSUTF8StringEncoding]
                                                           options:0 error:NULL];
                    if ([j isKindOfClass:[NSDictionary class]]) rel = j;
                }
                if (!rel) {
                    GMLog(@"cập nhật: không đọc được danh sách bản (%@) sau %lld ms",
                          err ? GMErrorText(err) : [NSString stringWithFormat:@"HTTP %@", GMVal(ok, @"code")], GMNowMs() - t0);
                    if (manual) [GMTheme toast:@"Không kiểm tra được bản mới — thử lại sau." in:GMTopView()];
                    return;
                }
                [self found:rel manual:manual];
            }];
}

+ (void)found:(NSDictionary *)rel manual:(BOOL)manual {
    NSString *tag = GMVal(rel, @"tag_name") ?: @"";
    NSString *version = [tag hasPrefix:@"v"] ? [tag substringFromIndex:1] : tag;
    NSDictionary *asset = nil, *sigAsset = nil;
    NSArray *assets = GMVal(rel, @"assets");
    for (NSDictionary *a in [assets isKindOfClass:[NSArray class]] ? assets : @[]) {
        NSString *name = GMVal(a, @"name") ?: @"";
        if ([name hasPrefix:@"vn.grammusic.app_"] && [name hasSuffix:@"_iphoneos-arm.deb"]) asset = a;
    }
    // Chữ ký của gói (tools/sign-release.sh): <tên gói>.sig — gmupdate chỉ cài gói có chữ ký đúng
    for (NSDictionary *a in [assets isKindOfClass:[NSArray class]] ? assets : @[]) {
        if (asset && [GMVal(a, @"name") isEqual:[GMVal(asset, @"name") stringByAppendingString:@".sig"]]) sigAsset = a;
    }
    NSString *cur = [self currentVersion];
    BOOL newer = asset && [self version:version newerThan:cur];
    GMLog(@"cập nhật: bản mới nhất %@ (đang dùng %@)%@", version, cur, newer ? @" — có bản mới" : @"");
    if (!newer) {
        if (manual) [GMTheme toast:[NSString stringWithFormat:@"Đang dùng bản mới nhất (%@).", cur] in:GMTopView()];
        return;
    }
    // «Để sau»: không hỏi lại bản này trong 24 giờ (trừ khi kiểm tay)
    NSDictionary *snooze = [[NSUserDefaults standardUserDefaults] dictionaryForKey:@"update.snooze"];
    if (!manual && [GMVal(snooze, @"version") isEqual:version] && [GMVal(snooze, @"until") doubleValue] > [[NSDate date] timeIntervalSince1970]) {
        return;
    }
    // Ghi chú phát hành (markdown): bỏ ** và `, «- » đầu dòng → «• »
    NSString *notes = GMVal(rel, @"body") ?: @"";
    notes = [[notes stringByReplacingOccurrencesOfString:@"**" withString:@""] stringByReplacingOccurrencesOfString:@"`" withString:@""];
    notes = [notes stringByReplacingOccurrencesOfString:@"\r" withString:@""];
    notes = [[@"\n" stringByAppendingString:notes] stringByReplacingOccurrencesOfString:@"\n- " withString:@"\n• "];
    notes = [notes stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
    if (notes.length > 500) notes = [[notes substringToIndex:500] stringByAppendingString:@"…"];
    NSString *msg = [NSString stringWithFormat:@"Đang dùng bản %@.%@%@\n\nCập nhật xong máy sẽ tự khởi động lại màn hình chính "
                                               @"(respring) rồi mở lại GramMusic.",
                                               cur, notes.length ? @"\n\n" : @"", notes];
    gPrompting = YES;
    GMAlertShow([NSString stringWithFormat:@"Có bản mới %@", version], msg, @"Để sau", @"Cập nhật", ^(NSInteger i) {
        gPrompting = NO;
        if (i == 0) {
            [[NSUserDefaults standardUserDefaults] setObject:@{@"version": version, @"until": @([[NSDate date] timeIntervalSince1970] + kSnooze)}
                                                      forKey:@"update.snooze"];
            return;
        }
        [self download:asset sig:sigAsset version:version];
    });
}

+ (NSString *)updateDir {
    return [NSHomeDirectory() stringByAppendingPathComponent:@"Documents/GramMusic-update"];
}

/// Có công cụ cài root chưa: chủ root + bit setuid
+ (BOOL)helperReady {
    struct stat st;
    return stat(kHelper.fileSystemRepresentation, &st) == 0 && st.st_uid == 0 && (st.st_mode & S_ISUID);
}

+ (void)download:(NSDictionary *)asset sig:(NSDictionary *)sigAsset version:(NSString *)version {
    if (gBusy) return;
    if (![self helperReady]) {
        GMAlertShow(@"Không tự cập nhật được", @"Thiếu công cụ cài (gmupdate). Cài bản mới một lần bằng Cydia, các bản sau sẽ tự cập nhật.",
                    @"OK", nil, nil);
        return;
    }
    NSString *sigURL = GMVal(sigAsset, @"browser_download_url");
    NSString *debURL = GMVal(asset, @"browser_download_url");
    if (![sigURL isKindOfClass:[NSString class]] || ![debURL isKindOfClass:[NSString class]] || ![sigURL hasPrefix:kAssetPrefix] ||
        ![debURL hasPrefix:kAssetPrefix]) {
        GMLog(@"cập nhật: bản %@ thiếu chữ ký hoặc link tải lạ", version);
        GMAlertShow(@"Không tự cập nhật được", @"Bản này chưa có chữ ký số. Cập nhật bằng Cydia (nguồn itovn.github.io/GramMusic/cydia).",
                    @"OK", nil, nil);
        return;
    }
    gBusy = YES;
    NSString *dir = [self updateDir];
    NSFileManager *fm = [NSFileManager defaultManager];
    [fm removeItemAtPath:dir error:NULL];
    [fm createDirectoryAtPath:dir withIntermediateDirectories:YES attributes:nil error:NULL];
    NSString *name = [GMVal(asset, @"name") lastPathComponent];
    // Tên riêng mỗi lượt: lượt đã huỷ (lõi vẫn tải nốt ở nền) không ghi đè tệp của lượt sau
    NSString *path = [dir stringByAppendingPathComponent:[NSString stringWithFormat:@"%@-%lld.deb", [name stringByDeletingPathExtension], GMNowMs()]];
    NSString *sigPath = [path stringByAppendingString:@".sig"];
    NSString *url = debURL;
    __block BOOL cancelled = NO, finished = NO;
    __block id observer = nil;
    GMBlockAlert *progress = GMAlertShow([NSString stringWithFormat:@"Đang tải bản %@…", version], @"0%", @"Huỷ", nil, ^(NSInteger i) {
        if (finished) return;
        cancelled = YES;
        gBusy = NO;
        GMLog(@"cập nhật: huỷ tải bản %@", version);
    });
    __weak GMBlockAlert *wp = progress;
    observer = [[NSNotificationCenter defaultCenter] addObserverForName:GMCoreEventNotification object:nil queue:nil
                                                             usingBlock:^(NSNotification *n) {
        NSDictionary *ev = n.userInfo;
        if (![GMVal(ev, @"e") isEqual:@"http.progress"] || ![GMVal(ev, @"url") isEqual:url]) return;
        double total = [GMVal(ev, @"total") doubleValue], done = [GMVal(ev, @"done") doubleValue];
        wp.message = total > 0 ? [NSString stringWithFormat:@"%.0f%% (%@ / %@)", done * 100 / total, [GMTheme sizeText:done], [GMTheme sizeText:total]]
                               : [GMTheme sizeText:done];
    }];
    long long t0 = GMNowMs();
    GMLog(@"cập nhật: tải %@", url);
    [GMCore call:@"http.download" args:@{@"url": url, @"path": path} done:^(id ok, NSDictionary *err) {
        [[NSNotificationCenter defaultCenter] removeObserver:observer];
        if (cancelled) {
            [fm removeItemAtPath:path error:NULL];
            return;
        }
        finished = YES;
        [progress dismissWithClickedButtonIndex:progress.cancelButtonIndex animated:YES];
        NSString *problem = err ? [@"Không tải được: " stringByAppendingString:GMErrorText(err)] : [self verify:ok asset:asset];
        GMLog(@"cập nhật: tải xong sau %lld ms%@", GMNowMs() - t0, problem ? [@" — " stringByAppendingString:problem] : @"");
        if (problem) {
            gBusy = NO;
            [fm removeItemAtPath:path error:NULL];
            GMAlertShow(@"Cập nhật không thành công", problem, @"OK", nil, nil);
            return;
        }
        // Chữ ký (vài trăm byte) — gmupdate kiểm trên bản chép của gói
        [GMCore call:@"http.download" args:@{@"url": sigURL, @"path": sigPath} done:^(id sok, NSDictionary *serr) {
            if (serr || [GMVal(sok, @"size") longLongValue] <= 0 || [GMVal(sok, @"size") longLongValue] > 1024) {
                gBusy = NO;
                [fm removeItemAtPath:path error:NULL];
                [fm removeItemAtPath:sigPath error:NULL];
                GMAlertShow(@"Cập nhật không thành công", @"Không tải được chữ ký của bản cập nhật.", @"OK", nil, nil);
                return;
            }
            [self install:path sig:sigPath version:version];
        }];
    }];
}

/// Kích thước + SHA-256 (GitHub ghi «digest»: "sha256:…" cho từng tệp đính kèm; lõi tính SHA-256 lúc tải); nil = đúng
+ (NSString *)verify:(NSDictionary *)r asset:(NSDictionary *)asset {
    long long got = [GMVal(r, @"size") longLongValue], want = [GMVal(asset, @"size") longLongValue];
    if (got <= 0) return @"Tệp tải về trống.";
    if (want > 0 && got != want) return @"Tệp tải về không đủ (sai kích thước).";
    NSString *digest = GMVal(asset, @"digest");
    if ([digest hasPrefix:@"sha256:"] && ![[digest substringFromIndex:7].lowercaseString isEqual:GMVal(r, @"sha256")]) {
        return @"Tệp tải về bị sai (SHA-256 không khớp).";
    }
    return nil;
}

+ (void)install:(NSString *)path sig:(NSString *)sigPath version:(NSString *)version {
    [GMCore call:@"app.flush" args:nil done:nil];
    NSString *pid = [NSString stringWithFormat:@"%d", getpid()];
    char *argv[] = {(char *)kHelper.fileSystemRepresentation, (char *)path.fileSystemRepresentation, (char *)pid.UTF8String,
                    (char *)sigPath.fileSystemRepresentation, NULL};
    pid_t child;
    int rc = posix_spawn(&child, argv[0], NULL, NULL, argv, environ);
    int status = 0;
    if (rc == 0) waitpid(child, &status, 0);
    int code = rc == 0 && WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    GMLog(@"cập nhật: gmupdate → %d (spawn %d)", code, rc);
    if (code != 0) {
        gBusy = NO;
        [[NSFileManager defaultManager] removeItemAtPath:[self updateDir] error:NULL];
        NSString *why = code == 66   ? @"công cụ cài thiếu quyền root"
                        : code == 68 ? @"tệp không phải gói của app"
                        : code == 72 ? @"chữ ký số không đúng"
                        : code == 73 ? @"bản này không mới hơn bản đang dùng"
                                     : @"lỗi khi kiểm tệp";
        GMAlertShow(@"Cập nhật không thành công", [NSString stringWithFormat:@"Không cài được (%@, mã %d).", why, code], @"OK", nil, nil);
        return;
    }
    // gmupdate đã tách nền: sắp đóng app, cài, respring, mở lại
    [[[UIAlertView alloc] initWithTitle:[NSString stringWithFormat:@"Đang cài bản %@…", version]
                                message:@"GramMusic sẽ tự đóng, máy khởi động lại màn hình chính rồi mở lại app." delegate:nil
                      cancelButtonTitle:nil otherButtonTitles:nil] show];
}

@end
