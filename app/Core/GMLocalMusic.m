#import "GMLocalMusic.h"
#import "GMLog.h"
#import <MediaPlayer/MediaPlayer.h>

static NSMutableDictionary *gSongs;  // khoá → dict (đọc lần trước)

@implementation GMLocalMusic

+ (BOOL)isLocalKey:(NSString *)key {
    return [key hasPrefix:@"ipod:"] || [key hasPrefix:@"file:"];
}

+ (BOOL)isIPodKey:(NSString *)key {
    return [key hasPrefix:@"ipod:"];
}

+ (MPMediaItem *)itemForKey:(NSString *)key {
    if (![self isIPodKey:key]) return nil;
    unsigned long long pid = strtoull([[key substringFromIndex:5] UTF8String], NULL, 10);
    MPMediaQuery *q = [[MPMediaQuery alloc] initWithFilterPredicates:nil];
    [q addFilterPredicate:[MPMediaPropertyPredicate predicateWithValue:@(pid) forProperty:MPMediaItemPropertyPersistentID]];
    NSArray *items = q.items;
    return items.count ? [items objectAtIndex:0] : nil;
}

+ (NSDictionary *)songFromItem:(MPMediaItem *)it {
    NSNumber *pid = [it valueForProperty:MPMediaItemPropertyPersistentID];
    NSURL *url = [it valueForProperty:MPMediaItemPropertyAssetURL];
    MPMediaItemArtwork *art = [it valueForProperty:MPMediaItemPropertyArtwork];
    NSString *ext = url ? url.pathExtension.lowercaseString : @"m4p";  // không có URL: thường là bài DRM của iTunes Store
    NSString *title = [it valueForProperty:MPMediaItemPropertyTitle] ?: @"";
    NSString *artist = [it valueForProperty:MPMediaItemPropertyArtist] ?: @"";
    NSString *album = [it valueForProperty:MPMediaItemPropertyAlbumTitle] ?: @"";
    NSNumber *dur = [it valueForProperty:MPMediaItemPropertyPlaybackDuration] ?: @0;
    return @{@"key": [NSString stringWithFormat:@"ipod:%llu", pid.unsignedLongLongValue], @"title": title, @"artist": artist,
             @"album": album, @"dur": @(round(dur.doubleValue)), @"ext": ext.length ? ext : @"m4a", @"ok": @(url != nil),
             @"thumb": @(art && art.bounds.size.width > 0), @"src": @"Nhạc trên máy", @"local": @YES};
}

+ (void)loadIPodSongs:(void (^)(NSArray *))done {
    dispatch_async(dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_DEFAULT, 0), ^{
        long long t = GMNowMs();
        NSMutableArray *out = [NSMutableArray array];
        for (MPMediaItem *it in [MPMediaQuery songsQuery].items) {
            if ([[it valueForProperty:MPMediaItemPropertyMediaType] integerValue] & MPMediaTypeAnyVideo) continue;
            [out addObject:[self songFromItem:it]];
        }
        [out sortUsingComparator:^NSComparisonResult(NSDictionary *a, NSDictionary *b) {
            return [[a objectForKey:@"title"] localizedCaseInsensitiveCompare:[b objectForKey:@"title"]];
        }];
        GMLog(@"nhạc trên máy: %lu bài (đọc %lld ms)", (unsigned long)out.count, GMNowMs() - t);
        dispatch_async(dispatch_get_main_queue(), ^{
            if (!gSongs) gSongs = [NSMutableDictionary dictionary];
            for (NSDictionary *s in out) [gSongs setObject:s forKey:[s objectForKey:@"key"]];
            done(out);
        });
    });
}

+ (NSDictionary *)songForKey:(NSString *)key {
    NSDictionary *s = [gSongs objectForKey:key];
    if (s) return s;
    MPMediaItem *it = [self itemForKey:key];
    if (!it) return nil;
    s = [self songFromItem:it];
    if (!gSongs) gSongs = [NSMutableDictionary dictionary];
    [gSongs setObject:s forKey:key];
    return s;
}

+ (NSURL *)urlForKey:(NSString *)key {
    return [[self itemForKey:key] valueForProperty:MPMediaItemPropertyAssetURL];
}

+ (UIImage *)artworkForKey:(NSString *)key pixels:(CGFloat)px {
    MPMediaItemArtwork *art = [[self itemForKey:key] valueForProperty:MPMediaItemPropertyArtwork];
    if (!art || art.bounds.size.width <= 0) return nil;
    return [art imageWithSize:CGSizeMake(px, px)];
}

+ (BOOL)playableFile:(NSString *)name {
    static NSSet *exts;
    if (!exts) {
        exts = [NSSet setWithArray:@[@"mp3", @"m4a", @"m4b", @"mp4", @"aac", @"wav", @"aif", @"aiff", @"caf", @"3gp", @"m4r", @"mov",
                                     @"flac", @"ogg", @"oga", @"opus"]];
    }
    return [exts containsObject:name.pathExtension.lowercaseString];
}

@end
