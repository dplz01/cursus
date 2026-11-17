# audiopus_sys 0.2.2 hardcodes <install-prefix>/lib in its linker search.
# GNUInstallDirs must use that same path on both Flatpak architectures.
set(CMAKE_INSTALL_LIBDIR "lib" CACHE PATH "Native library installation directory" FORCE)
