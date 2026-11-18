# Native Opus uses the same ABI and minimum API as the JNI library.
set(ANDROID_ABI "arm64-v8a" CACHE STRING "Android ABI" FORCE)
set(ANDROID_PLATFORM "android-26" CACHE STRING "Minimum Android API" FORCE)
set(CMAKE_INSTALL_LIBDIR "lib" CACHE PATH "Native library directory" FORCE)
if(DEFINED ENV{ANDROID_NDK_HOME})
  set(CURSUS_NDK "$ENV{ANDROID_NDK_HOME}")
elseif(DEFINED ENV{CARGO_NDK_SYSROOT_PATH})
  # cargo-ndk also discovers SDK-installed NDKs without ANDROID_NDK_HOME.
  get_filename_component(CURSUS_NDK "$ENV{CARGO_NDK_SYSROOT_PATH}/../../../../.." ABSOLUTE)
else()
  message(FATAL_ERROR "Build Android native libraries with cargo-ndk or set ANDROID_NDK_HOME")
endif()
include("${CURSUS_NDK}/build/cmake/android.toolchain.cmake")
