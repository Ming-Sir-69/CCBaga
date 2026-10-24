// [INPUT]: 虚构 argv 与本测试进程的显式资料参数。
// [OUTPUT]: 精确资料归属、重复参数拒绝及真实 sysctl argv 读取及窗口 PID 筛选回归。
// [POS]: 独立原生测试，不启动或关闭用户浏览器。
// [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
#import "Browser.m"
#include <assert.h>
int main(int argc, char **argv) {
    @autoreleasepool {
        NSString *profile = @"/test/独立 环境";
        assert(profileArgumentsMatch(@[@"Chromium", @"--user-data-dir=/test/独立 环境", @"about:blank"], profile));
        assert(profileArgumentsMatch(@[@"Chromium", @"--user-data-dir", profile], profile));
        assert(!profileArgumentsMatch(@[@"Chromium", @"--user-data-dir=/test/独立 环境-other"], profile));
        assert(!profileArgumentsMatch(@[@"Chromium", @"--user-data-dir=/other", @"--user-data-dir=/test/独立 环境"], profile));
        assert(!profileArgumentsMatch(@[@"Chromium", @"--", @"--user-data-dir=/test/独立 环境"], profile));
        assert(!profileArgumentsMatch(@[@"Chromium", @"--user-data-dir"], profile));
        assert(!profileArgumentsMatch(@[@"Chromium", @"https://example.org/?--user-data-dir=/test/独立 环境"], profile));
        NSArray<NSString *> *actual = profileArguments(getpid());
        assert(actual != nil);
        assert(profileArgumentsMatch(actual, profile));
        NSDictionary *bounds = CFBridgingRelease(CGRectCreateDictionaryRepresentation(CGRectMake(0, 0, 900, 700)));
        NSArray *windows = @[@{(__bridge NSString *)kCGWindowOwnerPID:@12,
                               (__bridge NSString *)kCGWindowLayer:@0,
                               (__bridge NSString *)kCGWindowIsOnscreen:@YES,
                               (__bridge NSString *)kCGWindowAlpha:@1,
                               (__bridge NSString *)kCGWindowBounds:bounds}];
        assert(profileWindowPresent(windows, 12));
        assert(!profileWindowPresent(windows, 13));
        assert(!profileWindowPresent(@[@{(__bridge NSString *)kCGWindowOwnerPID:@12,
                                         (__bridge NSString *)kCGWindowLayer:@0}], 12));
        NSMutableDictionary *helper = [windows[0] mutableCopy];
        [helper removeObjectForKey:(__bridge NSString *)kCGWindowIsOnscreen];
        assert(!profileWindowPresent(@[helper], 12));
        helper[(__bridge NSString *)kCGWindowIsOnscreen] = @YES;
        helper[(__bridge NSString *)kCGWindowAlpha] = @0;
        assert(!profileWindowPresent(@[helper], 12));
        helper[(__bridge NSString *)kCGWindowAlpha] = @1;
        helper[(__bridge NSString *)kCGWindowBounds] = CFBridgingRelease(CGRectCreateDictionaryRepresentation(CGRectMake(0, 0, 1920, 30)));
        assert(!profileWindowPresent(@[helper], 12));
        assert(browserProcessCandidate(@"/Applications/Brave Browser.app/Contents/Frameworks/Brave Browser Helper (Renderer)"));
        assert(!browserProcessCandidate(@"/usr/bin/sleep"));
        assert(profileArgumentsMatch(@[@"Chromium Helper", @"--type=renderer", @"--user-data-dir=/test/独立 环境"], profile));
        puts("Browser profile ownership/window/activity selection: 18 assertions passed");
    }
    return 0;
}
