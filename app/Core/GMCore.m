#import "GMCore.h"
#import "GMLog.h"
#import <UIKit/UIKit.h>
#include <string.h>
#include <sys/sysctl.h>

// core/crates/gm-ffi/src/lib.rs
typedef void (*gm_cb)(const char *json, void *ctx);
extern int gm_init(const char *config_json, gm_cb on_event, void *ctx);
extern void gm_call(const char *request_json, gm_cb on_result, void *ctx);

NSString *const GMCoreEventNotification = @"GMCoreEvent";

typedef void (^GMDone)(id ok, NSDictionary *err);

static NSString *GMJSONString(id obj) {
    NSData *d = obj ? [NSJSONSerialization dataWithJSONObject:obj options:0 error:NULL] : nil;
    return d ? [[NSString alloc] initWithData:d encoding:NSUTF8StringEncoding] : @"{}";
}

static id GMParse(const char *json) {
    if (!json) return nil;
    NSData *d = [NSData dataWithBytes:json length:strlen(json)];
    return [NSJSONSerialization JSONObjectWithData:d options:0 error:NULL];
}

static id GMUnNull(id v) {
    return v == [NSNull null] ? nil : v;
}

// Luồng của lõi; chuỗi chỉ dùng được trong lúc gọi → phân tích ngay rồi chuyển về luồng chính.
static void GMOnEvent(const char *json, void *ctx) {
    NSDictionary *ev = GMParse(json);
    if (![ev isKindOfClass:[NSDictionary class]]) return;
    dispatch_async(dispatch_get_main_queue(), ^{
        [[NSNotificationCenter defaultCenter] postNotificationName:GMCoreEventNotification object:nil userInfo:ev];
    });
}

static void GMOnResult(const char *json, void *ctx) {
    NSDictionary *r = GMParse(json);
    NSArray *h = (__bridge_transfer NSArray *)ctx;  // @[done, tên lệnh, lúc gọi]
    dispatch_async(dispatch_get_main_queue(), ^{
        GMDone done = [h objectAtIndex:0];
        long long ms = GMNowMs() - [[h objectAtIndex:2] longLongValue];
        id err = [r isKindOfClass:[NSDictionary class]] ? [r objectForKey:@"err"] : @{@"code": @"BAD_JSON", @"msg": @"Lõi trả kết quả không đọc được"};
        if ([err isKindOfClass:[NSDictionary class]]) {
            GMLog(@"[lõi] %@ lỗi %@ sau %lld ms", [h objectAtIndex:1], [err objectForKey:@"code"], ms);
            done(nil, err);
        } else {
            if (ms > 1500) GMLog(@"[lõi] %@ chậm: %lld ms", [h objectAtIndex:1], ms);
            done(GMUnNull([r objectForKey:@"ok"]), nil);
        }
    });
}

static NSString *GMMachine(void) {
    char buf[64] = {0};
    size_t len = sizeof(buf) - 1;
    if (sysctlbyname("hw.machine", buf, &len, NULL, 0) != 0) return @"iOS";
    return [NSString stringWithUTF8String:buf];
}

NSString *GMErrorText(NSDictionary *err) {
    NSString *msg = [err objectForKey:@"msg"];
    return [msg isKindOfClass:[NSString class]] && msg.length ? msg : @"Có lỗi.";
}

@implementation GMCore

+ (BOOL)start {
    static int result = -1;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        NSString *version = [[NSBundle mainBundle] objectForInfoDictionaryKey:@"CFBundleShortVersionString"] ?: @"0";
        NSDictionary *cfg = @{
            @"data_dir": GMDataDir(),
            @"device_model": GMMachine(),
            @"system_version": [@"iOS " stringByAppendingString:[UIDevice currentDevice].systemVersion],
            @"app_version": [@"GramMusic " stringByAppendingString:version],
            @"lang": @"vi",
        };
        long long t = GMNowMs();
        result = gm_init(GMJSONString(cfg).UTF8String, GMOnEvent, NULL);
        GMLog(@"[lõi] gm_init → %d sau %lld ms", result, GMNowMs() - t);
    });
    return result == 0;
}

+ (void)call:(NSString *)method args:(NSDictionary *)args done:(void (^)(id, NSDictionary *))done {
    NSArray *h = @[done ? [done copy] : [^(id ok, NSDictionary *err) {} copy], method, @(GMNowMs())];
    NSDictionary *req = @{@"m": method, @"a": args ?: @{}};
    gm_call(GMJSONString(req).UTF8String, GMOnResult, (__bridge_retained void *)h);
}

@end
