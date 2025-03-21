cmake_minimum_required(VERSION 3.12)

cmake_policy(SET CMP0091 NEW)

project(xtr
    VERSION 1.1.6
)

option(BUILD_EXAMPLES "Build with examples" OFF)

if(VCPKG_TARGET_TRIPLET MATCHES "arm64-linux")
    set(CARGO_CONFIG --config "target.aarch64-unknown-linux-gnu.linker='/usr/bin/aarch64-linux-gnu-gcc'")
    set(CARGO_TARGET "aarch64-unknown-linux-gnu")
    set(LINUX TRUE)
elseif(VCPKG_TARGET_TRIPLET MATCHES "x64-linux")
    set(CARGO_TARGET "x86_64-unknown-linux-gnu")
    set(LINUX TRUE)
else()
    if(MSVC)
        set(TOOLCHAIN "msvc")
    else()
        set(TOOLCHAIN "gnu")
    endif()

    if(NOT TRIPLET_SYSTEM_ARCH)
        if(CMAKE_SIZEOF_VOID_P EQUAL 8)
            set(TRIPLET_SYSTEM_ARCH "x64")
        elseif(CMAKE_SIZEOF_VOID_P EQUAL 4)
            set(TRIPLET_SYSTEM_ARCH "x86")
        endif()
    endif()

    if(TRIPLET_SYSTEM_ARCH STREQUAL "x86")
        set(CARGO_TARGET "i686-pc-windows-${TOOLCHAIN}")
    else()
        set(CARGO_TARGET "x86_64-pc-windows-${TOOLCHAIN}")
    endif()
endif()

if("Debug" MATCHES "${CMAKE_BUILD_TYPE}")
    set(CARGO_PROFILE "debug")
    set(CARGO_PROFILE_OPT "")
    set(XTR_CRATE_NAME "xtrd")
else()
    set(CARGO_PROFILE "release")
    set(CARGO_PROFILE_OPT "--release")
    set(XTR_CRATE_NAME "xtr")
endif()

if(VCPKG_TARGET_TRIPLET MATCHES "static")
    set(BUILD_SHARED_LIBS OFF)
else()
    set(BUILD_SHARED_LIBS ON)
endif()

if(VCPKG_CRT_LINKAGE MATCHES "static")
    set(RUSTFLAGS "-C target-feature=+crt-static")
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
else()
    set(RUSTFLAGS "-C target-feature=-crt-static")
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL")
endif()

set(CMAKE_VERBOSE_MAKEFILE TRUE)

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

find_program(CARGO cargo)

if(BUILD_SHARED_LIBS)
    set(XTR_STATIC OFF)

    add_library(xtr SHARED IMPORTED GLOBAL)

    if(WIN32)
        # Fix for Cross Compiling
        set(_PATH $ENV{PATH})
        list(FILTER _PATH EXCLUDE REGEX "Microsoft Visual Studio")

        add_custom_command(OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll"
            COMMAND ${CMAKE_COMMAND} -E env "PATH=${_PATH}" RUSTFLAGS=${RUSTFLAGS} ${CARGO} rustc --target=${CARGO_TARGET} ${CARGO_PROFILE_OPT} --crate-type cdylib
            DEPENDS xtr-copy-files
            WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
            VERBATIM
        )

        add_custom_command(OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll.lib"
            COMMAND ${CMAKE_COMMAND} -E env "PATH=${_PATH}" RUSTFLAGS=${RUSTFLAGS} ${CARGO} rustc --target=${CARGO_TARGET} ${CARGO_PROFILE_OPT} --crate-type cdylib
            DEPENDS xtr-copy-files
            WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
            VERBATIM
        )

        add_custom_target(xtr-cdylib
            ALL
            DEPENDS
                "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll"
                "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll.lib"
        )

        set_target_properties(xtr
            PROPERTIES
                IMPORTED_LOCATION "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll"
                IMPORTED_IMPLIB "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll.lib"
        )
    else()
        add_custom_command(OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION}"
            COMMAND ${CMAKE_COMMAND} -E env RUSTFLAGS=${RUSTFLAGS} ${CARGO} rustc --target=${CARGO_TARGET} ${CARGO_PROFILE_OPT} --crate-type cdylib ${CARGO_CONFIG}
            COMMAND ${CMAKE_COMMAND} -E rename "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so" "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION}"
            DEPENDS xtr-copy-files
            WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
            VERBATIM
        )
        add_custom_command(OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so"
            COMMAND ${CMAKE_COMMAND} -E create_symlink "lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION_MAJOR}" "lib${XTR_CRATE_NAME}.so"
            DEPENDS "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION_MAJOR}"
            WORKING_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}"
            VERBATIM
        )
        add_custom_command(OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION_MAJOR}"
            COMMAND ${CMAKE_COMMAND} -E create_symlink "lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION}" "lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION_MAJOR}"
            COMMAND ${CMAKE_COMMAND} -E create_symlink "lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION_MAJOR}" "lib${XTR_CRATE_NAME}.so"
            DEPENDS "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION}"
            WORKING_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}"
            VERBATIM
        )
        add_custom_target(xtr-cdylib
            ALL
            DEPENDS
                "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION_MAJOR}"
        )

        set_target_properties(xtr
            PROPERTIES
                IMPORTED_LOCATION "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION_MAJOR}"
                SKIP_BUILD_RPATH TRUE
                VERSION ${PROJECT_VERSION}
                SOVERSION ${PROJECT_VERSION_MAJOR}
        )
    endif()

    add_dependencies(xtr xtr-cdylib)
else()
    set(XTR_STATIC ON)

    add_library(xtr STATIC IMPORTED GLOBAL)

    if(WIN32)
        set_target_properties(xtr
            PROPERTIES
                IMPORTED_LOCATION "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.lib"
                # IMPORTED_IMPLIB "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.lib"
        )

        target_link_libraries(xtr
            INTERFACE
                bcrypt.lib
                ntdll.lib
                userenv.lib
                ws2_32.lib
        )

        # Fix for Cross Compiling
        set(_PATH $ENV{PATH})
        list(FILTER _PATH EXCLUDE REGEX "Microsoft Visual Studio")

        add_custom_command(OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.lib"
            COMMAND ${CMAKE_COMMAND} -E env "PATH=${_PATH}" RUSTFLAGS=${RUSTFLAGS} ${CARGO} rustc --target=${CARGO_TARGET} ${CARGO_PROFILE_OPT} --crate-type staticlib
            DEPENDS xtr-copy-files
            WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
            VERBATIM
        )

        add_custom_target(xtr-staticlib
            ALL
            DEPENDS
                "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.lib"
        )
    else()
        set_target_properties(xtr
            PROPERTIES
                IMPORTED_LOCATION "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.a"
                # IMPORTED_IMPLIB "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.lib"
        )

        add_custom_command(OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.a"
            COMMAND ${CMAKE_COMMAND} -E env RUSTFLAGS=${RUSTFLAGS} ${CARGO} rustc --target=${CARGO_TARGET} ${CARGO_PROFILE_OPT} --crate-type staticlib
            DEPENDS xtr-copy-files
            WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
            VERBATIM
        )

        add_custom_target(xtr-staticlib
            ALL
            DEPENDS
                "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.a"
        )
    endif()

    add_dependencies(xtr xtr-staticlib)
endif()

add_custom_target(xtr-copy-files
    COMMAND ${CMAKE_COMMAND} -E copy_directory_if_different ${CMAKE_CURRENT_SOURCE_DIR}/src ${CMAKE_CURRENT_BINARY_DIR}/src
)

add_library(xtr::xtr ALIAS xtr)

target_include_directories(xtr
    INTERFACE
        "$<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>/include"
        "$<INSTALL_INTERFACE:$<INSTALL_PREFIX>/include>"
)

if(BUILD_EXAMPLES)
    add_subdirectory(examples)
endif()

if(WIN32)
    if(BUILD_SHARED_LIBS)
        install(FILES
            "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll"
            "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.pdb"
            DESTINATION ${CMAKE_INSTALL_BINDIR}
        )
        install(FILES
            "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll.lib"
            RENAME "${XTR_CRATE_NAME}.lib"
            DESTINATION ${CMAKE_INSTALL_LIBDIR}
        )
    else()
        install(FILES
            "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.lib"
            DESTINATION ${CMAKE_INSTALL_LIBDIR}
        )
    endif()
else()
    if(BUILD_SHARED_LIBS)
        install(FILES
            "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION}"
            "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION_MAJOR}"
            "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so"
            DESTINATION ${CMAKE_INSTALL_LIBDIR}
        )
    else()
        install(FILES
            "${CMAKE_CURRENT_BINARY_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.a"
            DESTINATION ${CMAKE_INSTALL_LIBDIR}
        )
    endif()
endif()

install(DIRECTORY
    "${CMAKE_CURRENT_SOURCE_DIR}/include/xtr"
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR}
)

configure_package_config_file(${CMAKE_CURRENT_SOURCE_DIR}/cmake/xtrConfig.cmake.in
    "${CMAKE_CURRENT_BINARY_DIR}/xtrConfig.cmake"
    INSTALL_DESTINATION ${CMAKE_INSTALL_DATADIR}/xtr
)

configure_package_config_file(${CMAKE_CURRENT_SOURCE_DIR}/cmake/xtrTargets.cmake.in
    "${CMAKE_CURRENT_BINARY_DIR}/xtrTargets.cmake"
    INSTALL_DESTINATION ${CMAKE_INSTALL_DATADIR}/xtr
)

configure_package_config_file(${CMAKE_CURRENT_SOURCE_DIR}/cmake/xtrTargets-debug.cmake.in
    "${CMAKE_CURRENT_BINARY_DIR}/xtrTargets-debug.cmake"
    INSTALL_DESTINATION ${CMAKE_INSTALL_DATADIR}/xtr
)

configure_package_config_file(${CMAKE_CURRENT_SOURCE_DIR}/cmake/xtrTargets-release.cmake.in
    "${CMAKE_CURRENT_BINARY_DIR}/xtrTargets-release.cmake"
    INSTALL_DESTINATION ${CMAKE_INSTALL_DATADIR}/xtr
)

file(READ ${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml CARGO_TOML)
string(REPLACE "name = \"xtr\"" "name = \"${XTR_CRATE_NAME}\"" FILE_CONTENTS "${CARGO_TOML}")
file(WRITE ${CMAKE_CURRENT_BINARY_DIR}/Cargo.toml "${FILE_CONTENTS}")

write_basic_package_version_file(
    ${CMAKE_CURRENT_BINARY_DIR}/xtrConfigVersion.cmake
    VERSION ${PROJECT_VERSION}
    COMPATIBILITY SameMajorVersion
)

install(FILES
    ${CMAKE_CURRENT_BINARY_DIR}/xtrConfig.cmake
    ${CMAKE_CURRENT_BINARY_DIR}/xtrTargets.cmake
    ${CMAKE_CURRENT_BINARY_DIR}/xtrTargets-debug.cmake
    ${CMAKE_CURRENT_BINARY_DIR}/xtrTargets-release.cmake
    ${CMAKE_CURRENT_BINARY_DIR}/xtrConfigVersion.cmake
    DESTINATION ${CMAKE_INSTALL_DATADIR}/xtr
)

include(CPack)
