# glib/src/
> L2 | 父级: ../CLAUDE.md
成员清单
auto/: 上游 GIR 生成绑定；按生成代码维护来源，项目不手工扩大原生接口。
boxed.rs: 上游模块契约：`IMPL` Boxed wrapper implementation.
boxed_any_object.rs: 上游 boxed_any_object 绑定/回归实现，保留原公开接口与所有权约定。
boxed_inline.rs: 上游模块契约：`IMPL` BoxedInline wrapper implementation.
bridged_logging.rs: 上游 bridged_logging 绑定/回归实现，保留原公开接口与所有权约定。
byte_array.rs: 上游模块契约：# Examples
bytes.rs: 上游 bytes 绑定/回归实现，保留原公开接口与所有权约定。
char.rs: 上游 char 绑定/回归实现，保留原公开接口与所有权约定。
checksum.rs: 上游 checksum 绑定/回归实现，保留原公开接口与所有权约定。
clone.rs: 上游 clone 绑定/回归实现，保留原公开接口与所有权约定。
closure.rs: 上游 closure 绑定/回归实现，保留原公开接口与所有权约定。
collections/: 上游 GLib 容器所有权包装，保持外部 ABI 与释放规则。
control_flow.rs: 上游 control_flow 绑定/回归实现，保留原公开接口与所有权约定。
convert.rs: 上游 convert 绑定/回归实现，保留原公开接口与所有权约定。
date.rs: 上游 date 绑定/回归实现，保留原公开接口与所有权约定。
date_time.rs: 上游 date_time 绑定/回归实现，保留原公开接口与所有权约定。
enums.rs: 上游 enums 绑定/回归实现，保留原公开接口与所有权约定。
error.rs: 上游模块契约：`Error` binding and helper trait.
exit_code.rs: 上游 exit_code 绑定/回归实现，保留原公开接口与所有权约定。
future_with_timeout.rs: 上游 future_with_timeout 绑定/回归实现，保留原公开接口与所有权约定。
gobject/: 上游 GObject 对象与类型绑定，供 GLib 高层类型使用。
gstring.rs: 上游 gstring 绑定/回归实现，保留原公开接口与所有权约定。
gstring_builder.rs: 上游 gstring_builder 绑定/回归实现，保留原公开接口与所有权约定。
key_file.rs: 上游 key_file 绑定/回归实现，保留原公开接口与所有权约定。
lib.rs: 上游 lib 绑定/回归实现，保留原公开接口与所有权约定。
log.rs: 上游 log 绑定/回归实现，保留原公开接口与所有权约定。
main_context.rs: 上游 main_context 绑定/回归实现，保留原公开接口与所有权约定。
main_context_channel.rs: 上游 main_context_channel 绑定/回归实现，保留原公开接口与所有权约定。
main_context_futures.rs: 上游 main_context_futures 绑定/回归实现，保留原公开接口与所有权约定。
manual_functions.rs: 上游 manual_functions 绑定/回归实现，保留原公开接口与所有权约定。
object.rs: 上游模块契约：`IMPL` Object wrapper implementation and `Object` binding.
param_spec.rs: 上游 param_spec 绑定/回归实现，保留原公开接口与所有权约定。
prelude.rs: 上游模块契约：Traits and essential types intended for blanket imports.
property.rs: 上游 property 绑定/回归实现，保留原公开接口与所有权约定。
quark.rs: 上游 quark 绑定/回归实现，保留原公开接口与所有权约定。
shared.rs: 上游模块契约：`IMPL` Shared (reference counted) wrapper implementation.
signal.rs: 上游模块契约：`IMPL` Low level signal support.
source.rs: 上游 source 绑定/回归实现，保留原公开接口与所有权约定。
source_futures.rs: 上游 source_futures 绑定/回归实现，保留原公开接口与所有权约定。
subclass/: 上游 GObject 子类扩展契约，与 GLib 类型系统协作。
thread_guard.rs: 上游 thread_guard 绑定/回归实现，保留原公开接口与所有权约定。
thread_pool.rs: 上游 thread_pool 绑定/回归实现，保留原公开接口与所有权约定。
time_span.rs: 上游 time_span 绑定/回归实现，保留原公开接口与所有权约定。
time_zone.rs: 上游 time_zone 绑定/回归实现，保留原公开接口与所有权约定。
translate.rs: 上游模块契约：Translation between GLib/GLib-based FFI types and their Rust counterparts.
types.rs: 上游模块契约：Runtime type information.
unichar.rs: 上游 unichar 绑定/回归实现，保留原公开接口与所有权约定。
unicollate.rs: 上游 unicollate 绑定/回归实现，保留原公开接口与所有权约定。
utils.rs: 上游 utils 绑定/回归实现，保留原公开接口与所有权约定。
value.rs: 上游模块契约：`Value` binding and helper traits.
value_array.rs: 上游 value_array 绑定/回归实现，保留原公开接口与所有权约定。
variant.rs: 上游模块契约：`Variant` binding and helper traits.
variant_dict.rs: 上游 variant_dict 绑定/回归实现，保留原公开接口与所有权约定。
variant_iter.rs: Variant/字符串双端迭代器；唯一代码回补以可变引用交给 C 出参，避免优化构建的空指针未定义行为。
variant_type.rs: 上游 variant_type 绑定/回归实现，保留原公开接口与所有权约定。
win32.rs: 上游 win32 绑定/回归实现，保留原公开接口与所有权约定。
wrapper.rs: 上游模块契约：`IMPL` The `wrapper!` macro and miscellaneous wrapper traits.
[PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md
