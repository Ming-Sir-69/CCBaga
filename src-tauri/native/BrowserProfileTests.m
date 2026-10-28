// [INPUT]: 虚构 argv、合成进程信息、临时自建资料目录夹具、本测试进程自身 argv 与已回收子进程编号。
// [OUTPUT]: 唯一绝对参数、目录元数据与启动时间归属核验；锁类型异常及参数/元数据/进程读取失败保持未知，不退出其他资料的进程。
// [POS]: 独立原生测试；只在临时目录创建夹具，不读真实浏览器资料，不启动或结束浏览器，不枚举其他进程，不输出 argv。
// [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
#import "Browser.m"
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
static int passed = 0;
#define CHECK(...) do { if (!(__VA_ARGS__)) { fprintf(stderr, "FAILED %s:%d %s\n", __FILE__, __LINE__, #__VA_ARGS__); exit(1); } passed++; } while (0)
static NSDictionary *decodeResponse(char *raw) {
    NSDictionary *value = [NSJSONSerialization JSONObjectWithData:[NSData dataWithBytes:raw length:strlen(raw)] options:0 error:NULL];
    free(raw);
    return value;
}
static ProfileOwnership owner(NSString *argument, NSString *profile) {
    return profileDirectoryOwnership(profileArgument(@[@"Chromium", [@"--user-data-dir=" stringByAppendingString:argument]]), profile);
}
int main(int argc, char **argv) {
    @autoreleasepool {
        NSFileManager *files = NSFileManager.defaultManager;
        NSString *base = NSProcessInfo.processInfo.environment[@"CCBAGA_TEST_TMP"] ?: NSTemporaryDirectory();
        char *dir = strdup([[base stringByAppendingPathComponent:@"ccbaga-profile-XXXXXX"] fileSystemRepresentation]);
        CHECK(mkdtemp(dir) != NULL);
        NSString *root = [files stringWithFileSystemRepresentation:dir length:strlen(dir)];
        NSString *profile = [root stringByAppendingPathComponent:@"独立 环境"];
        NSString *other = [root stringByAppendingPathComponent:@"独立 环境-other"];
        NSString *link = [root stringByAppendingPathComponent:@"link"];
        CHECK([files createDirectoryAtPath:[profile stringByAppendingPathComponent:@"Default"] withIntermediateDirectories:YES attributes:nil error:NULL]);
        CHECK([files createDirectoryAtPath:other withIntermediateDirectories:NO attributes:nil error:NULL]);
        CHECK([files createSymbolicLinkAtPath:link withDestinationPath:profile error:NULL]);
        CHECK([@"fixture" writeToFile:[root stringByAppendingPathComponent:@"file"] atomically:NO encoding:NSUTF8StringEncoding error:NULL]);

        // ----- "--" 前仅一个绝对资料参数；其余歧义保持未知 -----
        CHECK([profileArgument(@[@"Chromium", @"--user-data-dir=/a", @"about:blank"]) isEqualToString:@"/a"]);
        CHECK([profileArgument(@[@"Chromium Helper", @"--type=renderer", @"--user-data-dir", @"/a"]) isEqualToString:@"/a"]);
        CHECK(profileArgument(@[@"Chromium", @"--user-data-dir=/a", @"--user-data-dir=/a"]) == nil);
        CHECK(profileArgument(@[@"Chromium", @"--user-data-dir=/b", @"--user-data-dir", @"/a"]) == nil);
        CHECK(profileArgument(@[@"Chromium", @"--user-data-dir"]) == nil);
        CHECK(profileArgument(@[@"Chromium", @"--user-data-dir="]) == nil);
        CHECK(profileArgument(@[@"Chromium", @"--user-data-dir=relative/a"]) == nil);
        CHECK(profileArgument(@[@"Chromium", @"--", @"--user-data-dir=/a"]) == nil);
        CHECK(profileArgument(@[@"Chromium", @"https://example.org/?--user-data-dir=/a"]) == nil);
        CHECK(profileArgument(@[@"Chromium", @"-user-data-dir=/a"]) == nil);
        CHECK(profileArgument(@[@"--user-data-dir=/a"]) == nil);
        CHECK(profileArgument(@[]) == nil);

        // ----- 同一目录的别名由设备/inode 识别 -----
        CHECK(owner(profile, profile) == ProfileOwnershipOwned);
        CHECK(owner([profile stringByAppendingString:@"/"], profile) == ProfileOwnershipOwned);
        CHECK(owner([profile stringByAppendingString:@"/./Default/.."], profile) == ProfileOwnershipOwned);
        CHECK(owner(link, profile) == ProfileOwnershipOwned);
        CHECK(owner(profile, [link stringByAppendingString:@"/"]) == ProfileOwnershipOwned);
        // 大小写别名仅在实际卷不区分大小写时验证。
        struct stat lower = {0}, upper = {0};
        NSString *upperRoot = [[root stringByDeletingLastPathComponent] stringByAppendingPathComponent:root.lastPathComponent.uppercaseString];
        if (stat(root.fileSystemRepresentation, &lower) == 0 && stat(upperRoot.fileSystemRepresentation, &upper) == 0 && lower.st_ino == upper.st_ino)
            CHECK(owner([upperRoot stringByAppendingPathComponent:@"独立 环境"], profile) == ProfileOwnershipOwned);
        // 相似前缀的独立目录不属于当前资料。
        CHECK(owner(other, profile) == ProfileOwnershipNone);
        CHECK(owner(root, profile) == ProfileOwnershipNone);
        // 元数据缺失、非目录和参数歧义均保持未知。
        CHECK(owner([root stringByAppendingPathComponent:@"missing"], profile) == ProfileOwnershipUnknown);
        CHECK(owner(profile, [root stringByAppendingPathComponent:@"missing"]) == ProfileOwnershipUnknown);
        CHECK(owner([root stringByAppendingPathComponent:@"file"], profile) == ProfileOwnershipUnknown);
        CHECK(owner(@"relative/独立 环境", profile) == ProfileOwnershipUnknown);
        CHECK(profileDirectoryOwnership(nil, profile) == ProfileOwnershipUnknown);

        // ----- 合成进程样本：二次读取失败或身份改变不能视为未运行 -----
        struct proc_bsdinfo before = {0}, same = {0}, reused = {0}, user = {0};
        before.pbi_uid = 501; before.pbi_start_tvsec = 100; before.pbi_start_tvusec = 7;
        same = before;
        reused = before; reused.pbi_start_tvusec = 8;
        user = before; user.pbi_uid = 502;
        NSArray *owned = @[@"Chromium", @"--user-data-dir", link], *foreign = @[@"Chromium", @"--user-data-dir", other];
        NSArray *ambiguous = @[@"Chromium", @"--user-data-dir", profile, @"--user-data-dir", profile], *missing = @[@"Chromium"];
        CHECK(profileOwnership(&before, YES, &same, owned, profile) == ProfileOwnershipOwned);
        CHECK(profileOwnership(&before, YES, &same, foreign, profile) == ProfileOwnershipNone);
        CHECK(profileOwnership(&before, YES, &same, ambiguous, profile) == ProfileOwnershipUnknown);
        CHECK(profileOwnership(&before, YES, &same, missing, profile) == ProfileOwnershipUnknown);
        CHECK(profileOwnership(&before, YES, &same, nil, profile) == ProfileOwnershipUnknown);
        CHECK(profileOwnership(&before, NO, &same, owned, profile) == ProfileOwnershipUnknown);
        CHECK(profileOwnership(&before, YES, &reused, owned, profile) == ProfileOwnershipUnknown);
        CHECK(profileOwnership(&before, YES, &user, owned, profile) == ProfileOwnershipUnknown);
        CHECK(missingProcess(ESRCH) == ProfileOwnershipNone);
        CHECK(missingProcess(EPERM) == ProfileOwnershipUnknown);
        CHECK(missingProcess(0) == ProfileOwnershipUnknown);
        for (NSString *lockValue in @[@"host-not-a-pid", @"host-", @"host-0", @"host-1", @"host-2147483648", @"host-999999999999999999999999", @"host-１２３"])
            CHECK(profileLockPID(lockValue) == -1);
        CHECK(profileLockPID(@"hyphenated-host-123") == 123);
        CHECK(profileApplicationIdentity(nil, @"Chromium") == ProfileOwnershipUnknown);
        CHECK(profileApplicationIdentity(@"ai.local.requirement-french.browser", nil) == ProfileOwnershipUnknown);
        CHECK(profileApplicationIdentity(@"", @"Chromium") == ProfileOwnershipUnknown);
        CHECK(profileApplicationIdentity(@"ai.local.requirement-french.browser", @"Chromium") == ProfileOwnershipOwned);
        CHECK(profileApplicationIdentity(@"org.fixture.other", @"Chromium") == ProfileOwnershipNone);
        CHECK(profileApplicationIdentity(@"ai.local.requirement-french.browser", @"other") == ProfileOwnershipNone);

        // ----- 真实进程读取仅限本测试及其已回收子进程；参数仅在内存比较 -----
        CHECK(argc == 2 && strncmp(argv[1], "--user-data-dir=", 16) == 0);
        NSArray<NSString *> *actual = profileArguments(getpid());
        CHECK(actual.count == (NSUInteger)argc);
        CHECK([profileArgument(actual) isEqualToString:[NSString stringWithUTF8String:argv[1] + 16]]);
        CHECK(profileArguments(-1) == nil);
        pid_t child = fork();
        if (child == 0) _exit(0);
        CHECK(child > 0 && waitpid(child, NULL, 0) == child);
        struct proc_bsdinfo gone = {0};
        errno = 0;
        CHECK(proc_pidinfo(child, PROC_PIDTBSDINFO, 0, &gone, sizeof(gone)) != sizeof(gone) && missingProcess(errno) == ProfileOwnershipNone);

        // 已回收 PID 明确未运行；测试进程缺应用元数据时保持未知，不授权退出。
        NSString *lock = [profile stringByAppendingPathComponent:@"SingletonLock"];
        CHECK([files createSymbolicLinkAtPath:lock withDestinationPath:[NSString stringWithFormat:@"host-%d", child] error:NULL]);
        CHECK([decodeResponse(rf_browser_running(profile.fileSystemRepresentation))[@"running"] isEqual:@NO]);
        CHECK([files removeItemAtPath:lock error:NULL]);
        NSString *selfLock = [NSString stringWithFormat:@"host-%d", getpid()];
        CHECK([files createSymbolicLinkAtPath:lock withDestinationPath:selfLock error:NULL]);
        CHECK(decodeResponse(rf_browser_running(profile.fileSystemRepresentation))[@"error"] != nil);
        CHECK(decodeResponse(rf_browser_quit(profile.fileSystemRepresentation))[@"error"] != nil);
        CHECK([files removeItemAtPath:lock error:NULL]);
        CHECK([files createSymbolicLinkAtPath:lock withDestinationPath:@"host-not-a-pid" error:NULL]);
        CHECK(decodeResponse(rf_browser_running(profile.fileSystemRepresentation))[@"error"] != nil);
        CHECK([files removeItemAtPath:lock error:NULL]);
        CHECK([@"unexpected regular lock" writeToFile:lock atomically:NO encoding:NSUTF8StringEncoding error:NULL]);
        CHECK(decodeResponse(rf_browser_running(profile.fileSystemRepresentation))[@"error"] != nil);
        CHECK(decodeResponse(rf_browser_quit(profile.fileSystemRepresentation))[@"error"] != nil);
        CHECK([files removeItemAtPath:lock error:NULL]);
        CHECK([decodeResponse(rf_browser_running(profile.fileSystemRepresentation))[@"running"] isEqual:@NO]);
        CHECK([files removeItemAtPath:root error:NULL]);
        free(dir);
        printf("Browser profile ownership: %d checks passed\n", passed);
    }
    return 0;
}
