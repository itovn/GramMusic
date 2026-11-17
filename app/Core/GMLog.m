#import "GMLog.h"
#include <sys/stat.h>
#include <sys/time.h>

NSString *GMDataDir(void) {
    static NSString *dir;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        dir = [NSHomeDirectory() stringByAppendingPathComponent:@"Library/GramMusic"];
        [[NSFileManager defaultManager] createDirectoryAtPath:dir withIntermediateDirectories:YES attributes:nil error:NULL];
    });
    return dir;
}

void GMTouchFile(NSString *path) {
    if (!path.length) return;
    const char *p = path.fileSystemRepresentation;
    struct stat st;
    if (stat(p, &st) != 0 || time(NULL) - st.st_mtime < 3600) return;
    utimes(p, NULL);
}

long long GMNowMs(void) {
    struct timeval tv;
    gettimeofday(&tv, NULL);
    return (long long)tv.tv_sec * 1000 + tv.tv_usec / 1000;
}

void GMLog(NSString *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    NSString *s = [[NSString alloc] initWithFormat:fmt arguments:ap];
    va_end(ap);
    NSLog(@"%@", s);
    static NSFileHandle *fh;
    static dispatch_queue_t q;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        q = dispatch_queue_create("gm.log", DISPATCH_QUEUE_SERIAL);
        NSString *path = [GMDataDir() stringByAppendingPathComponent:@"app.log"];
        NSFileManager *fm = [NSFileManager defaultManager];
        unsigned long long size = [[fm attributesOfItemAtPath:path error:NULL] fileSize];
        if (size > 1024 * 1024) [fm removeItemAtPath:path error:NULL];
        if (![fm fileExistsAtPath:path]) [fm createFileAtPath:path contents:nil attributes:nil];
        fh = [NSFileHandle fileHandleForWritingAtPath:path];
        [fh seekToEndOfFile];
    });
    long long ms = GMNowMs();
    dispatch_async(q, ^{
        NSString *line = [NSString stringWithFormat:@"%lld.%03lld %@\n", ms / 1000, ms % 1000, s];
        [fh writeData:[line dataUsingEncoding:NSUTF8StringEncoding]];
    });
}
