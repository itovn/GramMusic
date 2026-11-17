#import "GMSongCell.h"
#import "GMArt.h"
#import "GMImages.h"
#import "GMTheme.h"
#import <QuartzCore/QuartzCore.h>

@implementation GMSongCell {
    UIImageView *_cover, *_badge, *_dl;
    UILabel *_title, *_detail;
    UIButton *_moreBtn;
    NSString *_key;
}

+ (CGFloat)height {
    return 56;
}

- (instancetype)initWithStyle:(UITableViewCellStyle)style reuseIdentifier:(NSString *)rid {
    if ((self = [super initWithStyle:UITableViewCellStyleDefault reuseIdentifier:rid])) {
        _cover = [[UIImageView alloc] initWithFrame:CGRectMake(10, 6, 44, 44)];
        _cover.layer.cornerRadius = 4;
        _cover.clipsToBounds = YES;
        [self.contentView addSubview:_cover];
        _title = [[UILabel alloc] init];
        _title.font = [UIFont boldSystemFontOfSize:15];
        _title.backgroundColor = [UIColor clearColor];
        [self.contentView addSubview:_title];
        _detail = [[UILabel alloc] init];
        _detail.font = [UIFont systemFontOfSize:12.5];
        _detail.backgroundColor = [UIColor clearColor];
        [self.contentView addSubview:_detail];
        _badge = [[UIImageView alloc] init];
        [self.contentView addSubview:_badge];
        _dl = [[UIImageView alloc] init];
        [self.contentView addSubview:_dl];
        _moreBtn = [UIButton buttonWithType:UIButtonTypeCustom];
        [_moreBtn addTarget:self action:@selector(moreTapped) forControlEvents:UIControlEventTouchUpInside];
        [self.contentView addSubview:_moreBtn];
    }
    return self;
}

- (void)moreTapped {
    if (_more) _more();
}

- (void)layoutSubviews {
    [super layoutSubviews];
    CGFloat w = self.contentView.bounds.size.width;
    _moreBtn.frame = CGRectMake(w - 40, 0, 40, 56);
    CGFloat x = 62, right = w - 42;
    CGFloat bw = _badge.image ? _badge.image.size.width + 6 : 0;
    _title.frame = CGRectMake(x, 8, right - x - bw, 20);
    if (_badge.image) {
        CGSize s = _badge.image.size;
        _badge.frame = CGRectMake(right - s.width, 11, s.width, s.height);
    }
    CGFloat dx = _dl.image ? 16 : 0;
    _dl.frame = CGRectMake(x, 31, 12, 12);
    _detail.frame = CGRectMake(x + dx, 29, right - x - dx, 17);
}

- (void)setLoading {
    _key = nil;
    _title.text = @"…";
    _detail.text = @"";
    _cover.image = nil;
    _badge.image = nil;
    _dl.image = nil;
    _moreBtn.hidden = YES;
}

- (void)setSong:(NSDictionary *)s current:(BOOL)current progress:(double)progress showSource:(BOOL)showSource {
    NSString *key = GMVal(s, @"key");
    BOOL missing = [GMVal(s, @"missing") boolValue];
    _moreBtn.hidden = missing;
    [_moreBtn setImage:[GMArt glyph:@"more" size:20 color:[GMTheme secondaryText]] forState:UIControlStateNormal];
    _title.text = GMVal(s, @"title") ?: @"";
    _title.textColor = current ? [GMTheme accent] : (missing ? [GMTheme secondaryText] : [GMTheme text]);
    _detail.textColor = [GMTheme secondaryText];
    NSMutableArray *parts = [NSMutableArray array];
    NSString *artist = GMVal(s, @"artist");
    if (artist.length) [parts addObject:artist];
    double dur = [GMVal(s, @"dur") doubleValue];
    if (dur > 0) [parts addObject:GMDuration(dur)];
    NSString *src = GMVal(s, @"src");
    if (showSource && src.length) [parts addObject:src];
    if (missing) parts = [NSMutableArray arrayWithObject:@"Không còn trong thư viện"];
    if (progress >= 0 && progress < 1) [parts insertObject:[NSString stringWithFormat:@"Đang tải %.0f%%", progress * 100] atIndex:0];
    _detail.text = [parts componentsJoinedByString:@" · "];
    // Định dạng iOS 5 / 6 chưa phát được: nhãn đỏ
    BOOL ok = [GMVal(s, @"ok") boolValue];
    NSString *ext = [GMVal(s, @"ext") uppercaseString];
    _badge.image = (!ok && !missing && ext.length) ? [GMArt badge:ext color:[UIColor colorWithRed:0.85 green:0.3 blue:0.25 alpha:1]] : nil;
    NSString *dl = GMVal(s, @"dl");
    UIColor *green = [GMTheme checkColor];
    if ([dl isEqual:@"offline"]) _dl.image = [GMArt glyph:@"downloaded" size:12 color:green];
    else if ([dl isEqual:@"queued"]) _dl.image = [GMArt glyph:@"download" size:12 color:[GMTheme secondaryText]];
    else if ([dl isEqual:@"cached"]) _dl.image = [GMArt glyph:@"downloaded" size:12 color:[GMTheme secondaryText]];
    else _dl.image = nil;
    if (![key isEqual:_key]) {
        _key = key;
        _cover.image = key ? ([GMImages cachedCover:key size:44] ?: [GMArt placeholderFor:key size:44]) : nil;
        if (key) {
            [GMImages cover:s size:44 done:^(UIImage *img) {
                if ([_key isEqual:key]) _cover.image = img;
            }];
        }
    }
    [self setNeedsLayout];
}

@end
