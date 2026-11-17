#import "GMProxyViewController.h"
#import "GMCore.h"
#import "GMTheme.h"

@implementation GMProxyViewController {
    UITextField *_link;
    UILabel *_status;
    UIButton *_test;
    UIButton *_save;
    UIButton *_direct;
    UIActivityIndicatorView *_spin;
}

- (UILabel *)label:(CGRect)f size:(CGFloat)size {
    UILabel *l = [[UILabel alloc] initWithFrame:f];
    l.font = [UIFont systemFontOfSize:size];
    l.backgroundColor = [UIColor clearColor];
    l.numberOfLines = 0;
    l.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [self.view addSubview:l];
    return l;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Proxy";
    self.view.backgroundColor = [GMTheme background];
    [self.view insertSubview:[[GMWallpaperView alloc] initWithFrame:self.view.bounds] atIndex:0];
    CGFloat w = self.view.bounds.size.width;
    UILabel *hint = [self label:CGRectMake(20, 14, w - 40, 54) size:14];
    hint.textColor = [GMTheme secondaryText];
    hint.text = @"Dán link MTProxy (tg://proxy?server=…) hoặc SOCKS5 (tg://socks?…). Nhà mạng chặn Telegram thì cần proxy.";
    _link = [[UITextField alloc] initWithFrame:CGRectMake(20, 74, w - 40, 36)];
    _link.borderStyle = UITextBorderStyleRoundedRect;
    _link.font = [UIFont systemFontOfSize:14];
    _link.placeholder = @"tg://proxy?server=…&port=…&secret=…";
    _link.autocapitalizationType = UITextAutocapitalizationTypeNone;
    _link.autocorrectionType = UITextAutocorrectionTypeNo;
    _link.keyboardType = UIKeyboardTypeURL;
    _link.clearButtonMode = UITextFieldViewModeWhileEditing;
    _link.contentVerticalAlignment = UIControlContentVerticalAlignmentCenter;
    _link.delegate = self;
    _link.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [self.view addSubview:_link];
    CGFloat bw = (w - 50) / 2;
    _test = [GMTheme button:@"Thử kết nối"];
    _test.frame = CGRectMake(20, 122, bw, 42);
    _test.autoresizingMask = UIViewAutoresizingFlexibleRightMargin;
    [_test addTarget:self action:@selector(test) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:_test];
    _save = [GMTheme button:@"Lưu"];
    _save.frame = CGRectMake(30 + bw, 122, bw, 42);
    _save.autoresizingMask = UIViewAutoresizingFlexibleLeftMargin;
    [_save addTarget:self action:@selector(save) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:_save];
    _direct = [GMTheme button:@"Không dùng proxy (đi thẳng)"];
    [_direct setBackgroundImage:nil forState:UIControlStateNormal];  // nút phụ: chữ xanh, không nền
    [_direct setBackgroundImage:nil forState:UIControlStateHighlighted];
    [_direct setTitleColor:[GMTheme accent] forState:UIControlStateNormal];
    [_direct setTitleColor:[[GMTheme accent] colorWithAlphaComponent:0.4] forState:UIControlStateHighlighted];
    [_direct setTitleShadowColor:[UIColor clearColor] forState:UIControlStateNormal];
    _direct.frame = CGRectMake(20, 174, w - 40, 40);
    _direct.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [_direct addTarget:self action:@selector(direct) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:_direct];
    _spin = [[UIActivityIndicatorView alloc] initWithActivityIndicatorStyle:UIActivityIndicatorViewStyleGray];
    _spin.center = CGPointMake(w / 2, 238);
    _spin.hidesWhenStopped = YES;
    [self.view addSubview:_spin];
    _status = [self label:CGRectMake(20, 256, w - 40, 90) size:15];
    if (self.initialLink.length) {
        _link.text = self.initialLink;
        [self test];
        return;
    }
    [GMCore call:@"proxy.get" args:nil done:^(id ok, NSDictionary *err) {
        NSString *link = GMVal(ok, @"link");
        _link.text = link;
        _status.textColor = [GMTheme secondaryText];
        _status.text = link.length ? @"Đang dùng proxy này." : @"Đang đi thẳng (không proxy).";
    }];
}

- (BOOL)textFieldShouldReturn:(UITextField *)textField {
    [textField resignFirstResponder];
    return NO;
}

- (void)busy:(BOOL)busy {
    _test.enabled = _save.enabled = _direct.enabled = !busy;
    if (busy) [_spin startAnimating];
    else [_spin stopAnimating];
}

- (void)say:(NSString *)text ok:(BOOL)ok {
    _status.textColor = ok ? [UIColor colorWithRed:0.1 green:0.55 blue:0.2 alpha:1] : [UIColor colorWithRed:0.75 green:0.15 blue:0.1 alpha:1];
    _status.text = text;
}

- (NSString *)linkText {
    return [_link.text stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
}

- (void)test {
    [_link resignFirstResponder];
    [self busy:YES];
    _status.textColor = [GMTheme secondaryText];
    _status.text = @"Đang thử kết nối…";
    NSString *link = [self linkText];
    [GMCore call:@"proxy.test" args:@{@"link": link} done:^(id ok, NSDictionary *err) {
        [self busy:NO];
        if (err) [self say:GMErrorText(err) ok:NO];
        else [self say:[NSString stringWithFormat:@"Kết nối được (%@ ms). Bấm «Lưu» để dùng proxy này.", GMVal(ok, @"ms")] ok:YES];
    }];
}

- (void)saveLink:(NSString *)link {
    [_link resignFirstResponder];
    [self busy:YES];
    [GMCore call:@"proxy.set" args:@{@"link": link ?: [NSNull null]} done:^(id ok, NSDictionary *err) {
        [self busy:NO];
        if (err) [self say:GMErrorText(err) ok:NO];
        else [self say:link.length ? @"Đã lưu — kết nối mới đi qua proxy này." : @"Đã chuyển sang đi thẳng." ok:YES];
    }];
}

- (void)save {
    [self saveLink:[self linkText]];
}

- (void)direct {
    _link.text = @"";
    [self saveLink:nil];
}

@end
