/*
 * [INPUT]: 原生 System 桥、Foundation 地区目录与内存中的虚构语言/地区偏好。
 * [OUTPUT]: 只读地区预检、地区预设写入、后备语言保留与无效输入拒绝回归。
 * [POS]: 编译期替换全部偏好读写；不修改用户偏好、时区或定位。
 * [PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
 */
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <CoreLocation/CoreLocation.h>

static NSMutableDictionary *preferences;
static NSUInteger writes;
static NSUInteger synchronizations;
static BOOL synchronizeOK = YES;

static CFPropertyListRef fixtureCopy(CFStringRef key, CFStringRef app, CFStringRef user, CFStringRef host) {
    (void)app; (void)user; (void)host;
    return (__bridge_retained CFPropertyListRef)preferences[(__bridge NSString *)key];
}
static void fixtureSet(CFStringRef key, CFPropertyListRef value, CFStringRef app, CFStringRef user, CFStringRef host) {
    (void)app; (void)user; (void)host;
    writes++;
    preferences[(__bridge NSString *)key] = (__bridge id)value;
}
static Boolean fixtureSynchronize(CFStringRef app, CFStringRef user, CFStringRef host) {
    (void)app; (void)user; (void)host;
    synchronizations++;
    return synchronizeOK;
}
#define CFPreferencesCopyValue fixtureCopy
#define CFPreferencesSetValue fixtureSet
#define CFPreferencesSynchronize fixtureSynchronize
#include "System.m"
#undef CFPreferencesCopyValue
#undef CFPreferencesSetValue
#undef CFPreferencesSynchronize

static NSDictionary *json(char *response) {
    NSData *data = [[NSString stringWithUTF8String:response] dataUsingEncoding:NSUTF8StringEncoding];
    free(response);
    return [NSJSONSerialization JSONObjectWithData:data options:0 error:NULL];
}
static void reset(void) {
    preferences = [@{@"AppleLanguages": @[@"en-US", @"zh-Hans-CN", @"fr"], @"AppleLocale": @"en_US"} mutableCopy];
    writes = 0;
    synchronizations = 0;
    synchronizeOK = YES;
}
int main(void) { @autoreleasepool {
    // Exercise the production write path with fake preferences and the real OS locale catalog.
    NSArray *presets = @[
        @[@"en-US", @"en_US"], @[@"en-US", @"en_US"], @[@"en-CA", @"en_CA"],
        @[@"en-GB", @"en_GB"], @[@"fr", @"fr_FR"], @[@"de", @"de_DE"],
        @[@"ja", @"ja_JP"], @[@"ko", @"ko_KR"], @[@"en-SG", @"en_SG"],
        @[@"zh-Hant", @"zh_TW"], @[@"zh-Hant", @"zh_HK"],
        @[@"zh-Hans", @"zh_CN"], @[@"en-AU", @"en_AU"]
    ];
    NSUInteger failures = 0;
    for (NSArray *preset in presets) {
        reset();
        NSDictionary *before = [preferences copy];
        NSCAssert([json(rf_system_region_validate([preset[0] UTF8String], [preset[1] UTF8String]))[@"ok"] boolValue], @"catalog preset rejected by preflight: %@", preset);
        NSCAssert(writes == 0 && synchronizations == 0 && [preferences isEqual:before], @"preflight changed preferences");
        NSDictionary *result = json(rf_system_region([preset[0] UTF8String], [preset[1] UTF8String]));
        if (result[@"error"]) {
            fprintf(stderr, "FAIL %s / %s: %s\n", [preset[0] UTF8String], [preset[1] UTF8String], [result[@"error"] UTF8String]);
            failures++;
            continue;
        }
        NSCAssert(writes == 2 && synchronizations == 1, @"region write missing");
        NSCAssert([preferences[@"AppleLocale"] isEqual:preset[1]], @"catalog locale changed");
        NSMutableArray *expected = [@[@"en-US", @"zh-Hans-CN", @"fr"] mutableCopy];
        [expected removeObject:preset[0]];
        [expected insertObject:preset[0] atIndex:0];
        NSCAssert([preferences[@"AppleLanguages"] isEqual:expected], @"fallback languages changed");
        NSCAssert([result[@"state"][@"languages"] isEqual:expected], @"saved languages not read back");
    }
    for (NSArray *invalid in @[@[@"invalid", @"en_US"], @[@"en-US", @"invalid!"], @[@"zh-Hans", @"xx_YY"], @[@"zh-Hans", @""]]) {
        reset();
        NSDictionary *before = [preferences copy];
        NSCAssert(json(rf_system_region_validate([invalid[0] UTF8String], [invalid[1] UTF8String]))[@"error"] != nil, @"invalid region accepted by preflight");
        NSCAssert(json(rf_system_region([invalid[0] UTF8String], [invalid[1] UTF8String]))[@"error"] != nil, @"invalid region accepted");
        NSCAssert(writes == 0 && synchronizations == 0 && [preferences isEqual:before], @"invalid region changed preferences");
    }
    reset();
    synchronizeOK = NO;
    NSCAssert(json(rf_system_region("en-US", "en_US"))[@"error"] != nil, @"failed preference synchronization reported success");
    if (failures) return 1;
    puts("Isolated system region checks passed: read-only preflight, 13 presets, fallback languages, invalid inputs and synchronization failure.");
} return 0; }
