#import "GMLoginViewController.h"
#import "GMCore.h"
#import "GMLog.h"
#import "GMProxyViewController.h"
#import "GMTheme.h"
#import "GMAppDelegate.h"

typedef NS_ENUM(NSInteger, GMLoginStep) { GMStepPhone, GMStepCode, GMStepPassword };

@implementation GMLoginViewController {
    GMLoginStep _step;
    UILabel *_title;
    UILabel *_hint;
    UITextField *_field;
    UIButton *_go;
    UILabel *_status;
    UIActivityIndicatorView *_spin;
    NSString *_phone;
    NSInteger _gen;  // tăng khi lùi bước / đóng — bỏ kết quả của yêu cầu đang chạy dở
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Đăng nhập";
    self.view.backgroundColor = [GMTheme background];
    [self.view insertSubview:[[GMWallpaperView alloc] initWithFrame:self.view.bounds] atIndex:0];
    self.navigationItem.rightBarButtonItem =
        [[UIBarButtonItem alloc] initWithTitle:@"Proxy" style:UIBarButtonItemStyleBordered target:self action:@selector(proxy)];
    CGFloat w = self.view.bounds.size.width;
    _title = [[UILabel alloc] initWithFrame:CGRectMake(20, 20, w - 40, 24)];
    _title.font = [UIFont boldSystemFontOfSize:18];
    _title.backgroundColor = [UIColor clearColor];
    _title.textColor = [GMTheme text];
    _title.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [self.view addSubview:_title];
    _hint = [[UILabel alloc] initWithFrame:CGRectMake(20, 46, w - 40, 40)];
    _hint.font = [UIFont systemFontOfSize:14];
    _hint.textColor = [GMTheme secondaryText];
    _hint.numberOfLines = 2;
    _hint.backgroundColor = [UIColor clearColor];
    _hint.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [self.view addSubview:_hint];
    _field = [[UITextField alloc] initWithFrame:CGRectMake(20, 92, w - 40, 40)];
    _field.borderStyle = UITextBorderStyleRoundedRect;
    _field.font = [UIFont systemFontOfSize:20];
    _field.contentVerticalAlignment = UIControlContentVerticalAlignmentCenter;
    _field.clearButtonMode = UITextFieldViewModeWhileEditing;
    _field.delegate = self;
    _field.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [self.view addSubview:_field];
    _go = [GMTheme button:@""];
    _go.frame = CGRectMake(20, 144, w - 40, 44);
    _go.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [_go addTarget:self action:@selector(go) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:_go];
    _spin = [[UIActivityIndicatorView alloc] initWithActivityIndicatorStyle:UIActivityIndicatorViewStyleGray];
    _spin.center = CGPointMake(w / 2, 214);
    _spin.hidesWhenStopped = YES;
    _spin.autoresizingMask = UIViewAutoresizingFlexibleLeftMargin | UIViewAutoresizingFlexibleRightMargin;
    [self.view addSubview:_spin];
    _status = [[UILabel alloc] initWithFrame:CGRectMake(20, 234, w - 40, 80)];
    _status.font = [UIFont systemFontOfSize:14];
    _status.textColor = [UIColor colorWithRed:0.75 green:0.15 blue:0.1 alpha:1];
    _status.numberOfLines = 4;
    _status.backgroundColor = [UIColor clearColor];
    _status.autoresizingMask = UIViewAutoresizingFlexibleWidth;
    [self.view addSubview:_status];
    [self show:GMStepPhone hint:nil];
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    [_field becomeFirstResponder];
}

- (void)show:(GMLoginStep)step hint:(NSString *)hint {
    _step = step;
    // Bước số điện thoại: «Đóng» (thoát, đăng nhập sau); bước mã / mật khẩu: «Quay lại» nhập số khác
    self.navigationItem.leftBarButtonItem = step == GMStepPhone
        ? [[UIBarButtonItem alloc] initWithTitle:@"Đóng" style:UIBarButtonItemStyleBordered target:self action:@selector(close)]
        : [GMTheme backItem:@"Quay lại" target:self action:@selector(back)];
    _status.text = @"";
    _field.text = @"";
    _field.secureTextEntry = step == GMStepPassword;
    switch (step) {
        case GMStepPhone:
            _title.text = @"Số điện thoại";
            _hint.text = @"Dạng quốc tế, vd +84 912 345 678. Telegram sẽ gửi mã vào app Telegram trên máy khác của bạn.";
            _field.placeholder = @"+84…";
            _field.keyboardType = UIKeyboardTypePhonePad;
            [_go setTitle:@"Gửi mã" forState:UIControlStateNormal];
            break;
        case GMStepCode:
            _title.text = @"Mã đăng nhập";
            _hint.text = [NSString stringWithFormat:@"Mã đã gửi tới %@ (xem trong app Telegram trên máy khác).", _phone];
            _field.placeholder = @"Mã 5 số";
            _field.keyboardType = UIKeyboardTypeNumberPad;
            [_go setTitle:@"Đăng nhập" forState:UIControlStateNormal];
            break;
        case GMStepPassword:
            _title.text = @"Mật khẩu 2 lớp";
            _hint.text = hint.length ? [NSString stringWithFormat:@"Gợi ý: %@", hint] : @"Tài khoản này có bật mật khẩu 2 lớp.";
            _field.placeholder = @"Mật khẩu";
            _field.keyboardType = UIKeyboardTypeDefault;
            [_go setTitle:@"Xác nhận" forState:UIControlStateNormal];
            break;
    }
    [_field resignFirstResponder];
    [_field becomeFirstResponder];
}

/// Về bước số điện thoại (gõ nhầm số / đổi tài khoản) — mã / mật khẩu cũ bỏ; gửi mã lại từ đầu.
- (void)back {
    _gen++;
    [self busy:NO];
    [self show:GMStepPhone hint:nil];
    _field.text = _phone;
}

/// Thoát màn đăng nhập (đăng nhập sau: danh sách chat có nút «Đăng nhập»).
- (void)close {
    _gen++;
    [_field resignFirstResponder];
    [(GMAppDelegate *)[UIApplication sharedApplication].delegate loginClosed];
    [self dismissViewControllerAnimated:YES completion:nil];
}

- (void)debugStep:(NSInteger)step {
    if (step == GMStepCode && !_phone) _phone = @"+84…";
    [self show:(GMLoginStep)step hint:step == GMStepPassword ? @"(gợi ý thử)" : nil];
}

- (BOOL)textFieldShouldReturn:(UITextField *)textField {
    [self go];
    return NO;
}

- (void)busy:(BOOL)busy {
    _go.enabled = !busy;
    _field.enabled = !busy;
    // Đang kiểm mã / mật khẩu: «Quay lại» lúc này thì lõi vẫn đăng nhập xong mà màn này đã về bước đầu (kết quả bị bỏ). Bước số
    // điện thoại vẫn cho «Đóng» (gửi mã treo vì mạng thì vẫn thoát được)
    self.navigationItem.leftBarButtonItem.enabled = !busy || _step == GMStepPhone;
    if (busy) {
        _status.text = @"";
        [_spin startAnimating];
    } else {
        [_spin stopAnimating];
    }
}

- (void)go {
    NSString *v = [_field.text stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
    if (!v.length) return;
    [self busy:YES];
    NSInteger gen = _gen;
    if (_step == GMStepPhone) {
        NSString *phone = [[v componentsSeparatedByCharactersInSet:[NSCharacterSet characterSetWithCharactersInString:@" .-()"]]
            componentsJoinedByString:@""];
        if (![phone hasPrefix:@"+"]) phone = [@"+" stringByAppendingString:phone];
        _phone = phone;
        [GMCore call:@"auth.sendCode" args:@{@"phone": phone} done:^(id ok, NSDictionary *err) {
            if (gen != _gen) return;
            [self busy:NO];
            if (err) {
                _status.text = GMErrorText(err);
                return;
            }
            [self show:GMStepCode hint:nil];
        }];
    } else if (_step == GMStepCode) {
        [GMCore call:@"auth.signIn" args:@{@"code": v} done:^(id ok, NSDictionary *err) {
            if (gen != _gen) return;
            [self busy:NO];
            NSString *code = GMVal(err, @"code");
            if ([code isEqual:@"PASSWORD_NEEDED"]) {
                [self show:GMStepPassword hint:GMVal(err, @"hint")];
            } else if ([code isEqual:@"PHONE_CODE_EXPIRED"] || [code isEqual:@"NO_CODE"]) {
                // Mã hết hạn / lõi không còn lượt đăng nhập: thử lại cùng mã vô ích — về bước số điện thoại để xin mã mới
                [self show:GMStepPhone hint:nil];
                _field.text = _phone;
                _status.text = @"Mã đã hết hạn — bấm «Gửi mã» để nhận mã mới.";
            } else if (err) {
                _status.text = GMErrorText(err);
            } else {
                [self done];
            }
        }];
    } else {
        // Tính SRP của mật khẩu 2 lớp: iPhone 5 / iPad (A5, A6) mất ~6 giây
        UIColor *red = _status.textColor;
        _status.textColor = [GMTheme secondaryText];
        _status.text = @"Đang kiểm tra mật khẩu… máy cũ cần khoảng 5–10 giây.";
        [GMCore call:@"auth.checkPassword" args:@{@"password": v} done:^(id ok, NSDictionary *err) {
            if (gen != _gen) return;
            [self busy:NO];
            _status.textColor = red;
            if (err) {
                _status.text = GMErrorText(err);
                _field.text = @"";
            } else {
                [self done];
            }
        }];
    }
}

- (void)done {
    GMLog(@"đăng nhập xong");
    [_field resignFirstResponder];
    [self dismissViewControllerAnimated:YES completion:nil];
}

- (void)proxy {
    [self.navigationController pushViewController:[[GMProxyViewController alloc] init] animated:YES];
}

@end
