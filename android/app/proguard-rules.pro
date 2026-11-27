# Cursus release shrinking (R8). The default optimize rules already keep
# classes with native methods (chat.cursus.core.Native) under their names,
# which the Rust JNI symbols need, and kotlinx.serialization ships its own
# rules for @Serializable classes.

# The Rust side never calls back into Kotlin by name; keep the JNI entry
# points explicit anyway so a renamed class can't break library loading.
-keep class chat.cursus.core.Native { *; }

# Keep line numbers in crash reports.
-keepattributes SourceFile,LineNumberTable
-renamesourcefileattribute SourceFile
