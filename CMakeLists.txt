cmake_minimum_required(VERSION 3.12)

cmake_policy(SET CMP0091 NEW)

project(xtr
    VERSION 1.1.6
)

option(BUILD_EXAMPLES "Build with examples" OFF)
option(WITH_WIN7_COMPAT "Build with Windows 7 compatibility" OFF)

set(CMAKE_VERBOSE_MAKEFILE FALSE)

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
        if (WITH_WIN7_COMPAT)
            set(CARGO_TARGET "i686-win7-windows-${TOOLCHAIN}")
            set(CARGO_TOOLCHAIN "+nightly")
            set(CARGO_EXTRA_OPTS "-Zbuild-std")
        else()
            set(CARGO_TARGET "i686-pc-windows-${TOOLCHAIN}")
        endif()
    else()
        if (WITH_WIN7_COMPAT)
            set(CARGO_TARGET "x86_64-win7-windows-${TOOLCHAIN}")
            set(CARGO_TOOLCHAIN "+nightly")
            set(CARGO_EXTRA_OPTS "-Zbuild-std")
        else()
            set(CARGO_TARGET "x86_64-pc-windows-${TOOLCHAIN}")
        endif()
    endif()
endif()

set(XTR_CRATE_NAME "xtr")
set(CARGO_PROFILE "$<IF:$<CONFIG:Debug>,debug,release>")
set(CARGO_PROFILE_OPT "$<IF:$<CONFIG:Debug>,-v,--release>")

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

include(GNUInstallDirs)

add_library(${PROJECT_NAME})
add_library(fv::${PROJECT_NAME} ALIAS ${PROJECT_NAME})
add_library(${PROJECT_NAME}::${PROJECT_NAME} ALIAS ${PROJECT_NAME})

target_sources(${PROJECT_NAME}
    PRIVATE
        src/dummy.cpp
)

target_include_directories(${PROJECT_NAME}
    INTERFACE
        "$<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>/include"
        "$<INSTALL_INTERFACE:include>"
)

set_target_properties(${PROJECT_NAME}
    PROPERTIES
        SKIP_BUILD_RPATH TRUE
        VERSION ${PROJECT_VERSION}
        SOVERSION ${PROJECT_VERSION_MAJOR}
)

# Build the actual library
find_program(CARGO cargo)

if(BUILD_SHARED_LIBS)
    if(WIN32)
        # Fix for Cross Compiling
        set(_PATH $ENV{PATH})
        list(FILTER _PATH EXCLUDE REGEX "Microsoft Visual Studio")

        add_custom_command(OUTPUT "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll"
            COMMAND ${CMAKE_COMMAND} -E env "PATH=${_PATH}" CARGO_NET_GIT_FETCH_WITH_CLI=true RUSTFLAGS=${RUSTFLAGS} ${CARGO} ${CARGO_TOOLCHAIN} rustc --target=${CARGO_TARGET} ${CARGO_PROFILE_OPT} ${CARGO_EXTRA_OPTS} --crate-type cdylib
            WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
            VERBATIM
        )

        add_custom_command(OUTPUT "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll.lib"
            COMMAND ${CMAKE_COMMAND} -E env "PATH=${_PATH}" CARGO_NET_GIT_FETCH_WITH_CLI=true RUSTFLAGS=${RUSTFLAGS} ${CARGO} ${CARGO_TOOLCHAIN} rustc --target=${CARGO_TARGET} ${CARGO_PROFILE_OPT} ${CARGO_EXTRA_OPTS} --crate-type cdylib
            WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
            VERBATIM
        )

        add_custom_target(${PROJECT_NAME}-cdylib
            ALL
            DEPENDS
                "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll"
                "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll.lib"
        )

        add_custom_command(TARGET ${PROJECT_NAME}
            POST_BUILD
                COMMAND ${CMAKE_COMMAND} -E echo "-- Replace $<TARGET_FILE:${PROJECT_NAME}> with ${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll"
                COMMAND ${CMAKE_COMMAND} -E copy "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll" "$<TARGET_FILE:${PROJECT_NAME}>"
                COMMAND ${CMAKE_COMMAND} -E echo "-- Replace $<TARGET_IMPORT_FILE:${PROJECT_NAME}> with ${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll.lib"
                COMMAND ${CMAKE_COMMAND} -E copy "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.dll.lib" "$<TARGET_IMPORT_FILE:${PROJECT_NAME}>"
        )

        if(MSVC)
            add_custom_command(TARGET ${PROJECT_NAME}
                POST_BUILD
                    COMMAND ${CMAKE_COMMAND} -E echo "-- Replace $<TARGET_PDB_FILE:${PROJECT_NAME}> with ${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.pdb"
                    COMMAND ${CMAKE_COMMAND} -E copy "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.pdb" "$<TARGET_PDB_FILE:${PROJECT_NAME}>"
            )
        endif()
    else()
        add_custom_command(OUTPUT "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION}"
            COMMAND ${CMAKE_COMMAND} -E env CARGO_NET_GIT_FETCH_WITH_CLI=true RUSTFLAGS=${RUSTFLAGS} ${CARGO} ${CARGO_TOOLCHAIN} rustc --target=${CARGO_TARGET} ${CARGO_PROFILE_OPT} ${CARGO_EXTRA_OPTS} --crate-type cdylib ${CARGO_CONFIG}
            COMMAND ${CMAKE_COMMAND} -E rename "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so" "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION}"
            WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
            VERBATIM
        )
        add_custom_command(TARGET ${PROJECT_NAME}
            POST_BUILD
                COMMAND ${CMAKE_COMMAND} -E echo "-- Replace $<TARGET_FILE:${PROJECT_NAME}> with ${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION}"
                COMMAND ${CMAKE_COMMAND} -E copy "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION}" "$<TARGET_FILE:${PROJECT_NAME}>"
        )
        add_custom_target(${PROJECT_NAME}-cdylib
            ALL
            DEPENDS
                "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.so.${PROJECT_VERSION}"
        )
    endif()

    add_dependencies(${PROJECT_NAME} ${PROJECT_NAME}-cdylib)
else()
    if(WIN32)
        target_link_libraries(${PROJECT_NAME}
            INTERFACE
                bcrypt.lib
                ntdll.lib
                userenv.lib
                ws2_32.lib
        )

        # Fix for Cross Compiling
        set(_PATH $ENV{PATH})
        list(FILTER _PATH EXCLUDE REGEX "Microsoft Visual Studio")

        add_custom_command(OUTPUT "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.lib"
            COMMAND ${CMAKE_COMMAND} -E env "PATH=${_PATH}" CARGO_NET_GIT_FETCH_WITH_CLI=true RUSTFLAGS=${RUSTFLAGS} ${CARGO} ${CARGO_TOOLCHAIN} rustc --target=${CARGO_TARGET} ${CARGO_PROFILE_OPT} ${CARGO_EXTRA_OPTS} --crate-type staticlib
            WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
            VERBATIM
        )

        add_custom_command(TARGET ${PROJECT_NAME}
            POST_BUILD
                COMMAND ${CMAKE_COMMAND} -E echo "-- Replace $<TARGET_FILE:${PROJECT_NAME}> with ${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.lib"
                COMMAND ${CMAKE_COMMAND} -E copy "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.lib" "$<TARGET_FILE:${PROJECT_NAME}>"
        )

        add_custom_target(${PROJECT_NAME}-staticlib
            ALL
            DEPENDS
                "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/${XTR_CRATE_NAME}.lib"
        )
    else()
        add_custom_command(OUTPUT "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.a"
            COMMAND ${CMAKE_COMMAND} -E env CARGO_NET_GIT_FETCH_WITH_CLI=true RUSTFLAGS=${RUSTFLAGS} ${CARGO} ${CARGO_TOOLCHAIN} rustc --target=${CARGO_TARGET} ${CARGO_PROFILE_OPT} ${CARGO_EXTRA_OPTS} --crate-type staticlib
            WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
            VERBATIM
        )
        add_custom_command(TARGET ${PROJECT_NAME}
            POST_BUILD
                COMMAND ${CMAKE_COMMAND} -E echo "-- Replace $<TARGET_FILE:${PROJECT_NAME}> with ${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.a"
                COMMAND ${CMAKE_COMMAND} -E copy "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.a" "$<TARGET_FILE:${PROJECT_NAME}>"
        )
        add_custom_target(${PROJECT_NAME}-staticlib
            ALL
            DEPENDS
                "${CMAKE_CURRENT_SOURCE_DIR}/target/${CARGO_TARGET}/${CARGO_PROFILE}/lib${XTR_CRATE_NAME}.a"
        )
    endif()

    add_dependencies(${PROJECT_NAME} ${PROJECT_NAME}-staticlib)
endif()

if(BUILD_EXAMPLES)
    add_subdirectory(examples)
endif()

include(CMakePackageConfigHelpers)

# Install the shared library and headers
install(
    DIRECTORY include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR} COMPONENT Development
)

# Install the library and export the targets
install(TARGETS ${PROJECT_NAME}
    EXPORT ${PROJECT_NAME}-targets
    ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR} COMPONENT Development
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR} COMPONENT Development
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR} COMPONENT Runtime
)

# Install the debug symbols if available
if(BUILD_SHARED_LIBS AND MSVC)
    install(FILES
        $<TARGET_PDB_FILE:${PROJECT_NAME}>
        DESTINATION ${CMAKE_INSTALL_BINDIR}
        COMPONENT Development
        OPTIONAL
    )
endif()

# Final export and create the config file
export(EXPORT ${PROJECT_NAME}-targets
    FILE ${PROJECT_NAME}-targets.cmake
    NAMESPACE ${PROJECT_NAME}::
)

# Install the export set
install(EXPORT ${PROJECT_NAME}-targets
    FILE ${PROJECT_NAME}-targets.cmake
    DESTINATION ${CMAKE_INSTALL_DATADIR}/${PROJECT_NAME} COMPONENT Development
    NAMESPACE ${PROJECT_NAME}::
)

# Create the config file for the package
include(CMakePackageConfigHelpers)
configure_package_config_file(
    cmake/Config.cmake.in
    ${CMAKE_CURRENT_BINARY_DIR}/${PROJECT_NAME}-config.cmake
    INSTALL_DESTINATION ${CMAKE_INSTALL_DATADIR}/${PROJECT_NAME}
)

# Create the version file for the package
write_basic_package_version_file(
    ${CMAKE_CURRENT_BINARY_DIR}/${PROJECT_NAME}-config-version.cmake
    VERSION ${PROJECT_VERSION}
    COMPATIBILITY SameMajorVersion
)

# Install the config and version files
install(FILES
    ${CMAKE_CURRENT_BINARY_DIR}/${PROJECT_NAME}-config.cmake
    ${CMAKE_CURRENT_BINARY_DIR}/${PROJECT_NAME}-config-version.cmake
    DESTINATION ${CMAKE_INSTALL_DATADIR}/${PROJECT_NAME} COMPONENT Development
)

include(CPack)
