# Copyright © SixtyFPS GmbH <info@slint-ui.com>
# SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-commercial

cmake_minimum_required(VERSION 3.19)
project(Slint HOMEPAGE_URL "https://slint-ui.com/" LANGUAGES CXX)

include(FeatureSummary)
include(CMakeDependentOption)

include(FetchContent)
FetchContent_Declare(
    Corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG v0.2.1
)
FetchContent_MakeAvailable(Corrosion)

list(PREPEND CMAKE_MODULE_PATH ${Corrosion_SOURCE_DIR}/cmake)
find_package(Rust 1.56 REQUIRED MODULE)

option(SLINT_FEATURE_COMPILER "Enable support for compiling .slint files to C++ ahead of time" ON)

if (SLINT_FEATURE_COMPILER)
    set(slint_compiler_crate "slint-compiler")
endif()

corrosion_import_crate(MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../Cargo.toml"
    CRATES slint-cpp ${slint_compiler_crate})


set_property(
    TARGET slint-cpp
    APPEND
    PROPERTY CORROSION_ENVIRONMENT_VARIABLES
    SLINT_GENERATED_INCLUDE_DIR="${CMAKE_CURRENT_BINARY_DIR}/generated_include/"
)

if (SLINT_FEATURE_COMPILER)
    set_property(
        TARGET slint-compiler
        PROPERTY CORROSION_USE_HOST_BUILD 1
    )
endif()

add_library(Slint INTERFACE)
add_library(Slint::Slint ALIAS Slint)
target_link_libraries(Slint INTERFACE slint-cpp)
target_compile_features(Slint INTERFACE cxx_std_20)

function(define_cargo_feature cargo-feature description default)
    # turn foo-bar into SLINT_FEATURE_FOO_BAR
    string(TOUPPER "${cargo-feature}" cmake_option)
    string(REPLACE "-"  "_" cmake_option "${cmake_option}")
    set(cmake_option "SLINT_FEATURE_${cmake_option}")
    option("${cmake_option}" "${description}" ${default})
    if(${cmake_option})
        list(APPEND features ${cargo-feature})
    endif()
    set(features "${features}" PARENT_SCOPE)
    add_feature_info(${cmake_option} ${cmake_option} ${description})
endfunction()

function(define_cargo_dependent_feature cargo-feature description default depends_condition)
    # turn foo-bar into SLINT_FEATURE_FOO_BAR
    string(TOUPPER "${cargo-feature}" cmake_option)
    string(REPLACE "-"  "_" cmake_option "${cmake_option}")
    set(cmake_option "SLINT_FEATURE_${cmake_option}")
    cmake_dependent_option("${cmake_option}" "${description}" ${default} ${depends_condition} OFF)
    if(${cmake_option})
        list(APPEND features ${cargo-feature})
    endif()
    set(features "${features}" PARENT_SCOPE)
    add_feature_info(${cmake_option} ${cmake_option} ${description})
endfunction()

# Features that are mapped to features in the Rust crate. These and their
# defaults need to be kept in sync with the Rust bit.
define_cargo_feature(interpreter "Enable support for the Slint interpeter to load .slint files at run-time" ON)

define_cargo_feature(backend-gl-all "Enable OpenGL ES 2.0 based rendering backend with support for all windowing systems." ON)
define_cargo_feature(backend-gl-x11 "Enable the OpenGL ES 2.0 backend with only X11 support on Unix. Enable this option and turn off SLINT_FEATURE_BACKEND_GL_ALL for a smaller build with just X11 support on Unix." OFF)
define_cargo_feature(backend-gl-wayland "Enable the OpenGL ES 2.0 backend with wayland support on Unix. Enable this option and turn off SLINT_FEATURE_BACKEND_GL_ALL for a smaller build with just wayland support." OFF)

define_cargo_feature(backend-qt "Enable Qt based rendering backend" ON)

if(SLINT_FEATURE_INTERPRETER)
    if (SLINT_FEATURE_BACKEND_QT)
        list(APPEND features rtti-qt)
    endif()
    if(SLINT_FEATURE_BACKEND_GL_ALL OR SLINT_FEATURE_BACKEND_GL_X11 OR SLINT_FEATURE_BACKEND_GL_WAYLAND)
        list(APPEND features rtti-gl)
    endif()
endif()

set_property(
    TARGET slint-cpp
    PROPERTY CORROSION_FEATURES
    ${features}
)
set_property(
    TARGET slint-cpp
    PROPERTY CORROSION_NO_DEFAULT_FEATURES
    ${features}
)


if (SLINT_FEATURE_BACKEND_QT)
    # For the CMake build don't rely on qmake being in PATH but use CMake to locate Qt. This
    # means usually CMAKE_PREFIX_PATH is set.
    find_package(Qt6 6.0 QUIET COMPONENTS Core Widgets)
    if (NOT TARGET Qt::qmake)
        find_package(Qt5 5.15 QUIET COMPONENTS Core Widgets)
    endif()
endif (SLINT_FEATURE_BACKEND_QT)
if (TARGET Qt::qmake)
    set_property(
        TARGET slint-cpp
        APPEND
        PROPERTY CORROSION_ENVIRONMENT_VARIABLES
        QMAKE=$<TARGET_PROPERTY:Qt::qmake,LOCATION>
    )
    set(SLINT_STYLE_DEFAULT "native")
else()
    set_property(
        TARGET slint-cpp
        APPEND
        PROPERTY CORROSION_ENVIRONMENT_VARIABLES
        SLINT_NO_QT=1
    )
    set(SLINT_STYLE_DEFAULT "fluent")
endif()

set(SLINT_STYLE ${SLINT_STYLE_DEFAULT} CACHE STRING "The Slint widget style" FORCE)

file(GLOB api_headers RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}/include/"
    "${CMAKE_CURRENT_SOURCE_DIR}/include/*.h")

foreach(header IN LISTS api_headers)
    set_property(TARGET Slint APPEND PROPERTY PUBLIC_HEADER include/${header})
endforeach()

set(generated_headers
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_enums_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_string_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_brush_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_sharedvector_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_properties_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_image_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_color_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_pathdata_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_qt_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_selector_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_backend_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_generated_public.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_interpreter_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/slint_interpreter_generated_public.h
)

foreach(header IN LISTS generated_headers)
    set_property(TARGET Slint APPEND PROPERTY PUBLIC_HEADER ${header})
endforeach()

target_include_directories(Slint INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/generated_include>
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include/slint>
)

if (SLINT_FEATURE_COMPILER)
    add_executable(Slint::slint-compiler ALIAS slint-compiler)
    include(${CMAKE_CURRENT_LIST_DIR}/cmake/SlintMacro.cmake)
endif()

export(TARGETS Slint slint-cpp
    NAMESPACE Slint:: FILE "${CMAKE_BINARY_DIR}/lib/cmake/Slint/SlintTargets.cmake")
install(EXPORT SlintTargets NAMESPACE Slint:: DESTINATION lib/cmake/Slint)
install(TARGETS Slint slint-cpp
    EXPORT SlintTargets  LIBRARY DESTINATION lib PUBLIC_HEADER DESTINATION include/slint)

include(CMakePackageConfigHelpers)
include(GNUInstallDirs)

install(FILES $<TARGET_FILE:slint-cpp-shared> TYPE LIB)
if(WIN32)
    install(FILES $<TARGET_LINKER_FILE:slint-cpp-shared> TYPE LIB)
    # Copy the dll to the top-level bin directory, where the examples will
    # will also be located, so that they can find the dll.
    get_property(GENERATOR_IS_MULTI_CONFIG GLOBAL PROPERTY GENERATOR_IS_MULTI_CONFIG)
    if(GENERATOR_IS_MULTI_CONFIG)
        set(config_subdir_genex "$<CONFIG>/")
    endif()
    add_custom_target(Slint_dll_convenience ALL DEPENDS slint-cpp-shared
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            $<TARGET_FILE:slint-cpp-shared>
	    ${CMAKE_BINARY_DIR}/bin/${config_subdir_genex}$<TARGET_FILE_NAME:slint-cpp-shared>)
endif()

if (SLINT_FEATURE_COMPILER)
    install(PROGRAMS $<TARGET_FILE:slint-compiler> TYPE BIN)
endif()

set(SLINT_LIB_PROPERTIES "IMPORTED_NO_SONAME;1")
foreach(prop
        IMPORTED_LOCATION IMPORTED_LOCATION_DEBUG IMPORTED_LOCATION_RELEASE
        IMPORTED_LOCATION_RELWITHDEBINFO IMPORTED_LOCATION_MINSIZEREL
        IMPORTED_IMPLIB IMPORTED_IMPLIB_DEBUG IMPORTED_IMPLIB_RELEASE
        IMPORTED_IMPLIB_RELWITHDEBINFO IMPORTED_IMPLIB_MINSIZEREL)
  get_target_property(value slint-cpp-shared ${prop})
  if(value)
    get_filename_component(value ${value} NAME)
    list(APPEND SLINT_LIB_PROPERTIES ${prop} "\${_IMPORT_PREFIX}/${CMAKE_INSTALL_LIBDIR}/${value}")
  endif()
endforeach()

configure_package_config_file("cmake/SlintConfig.cmake.in" "${CMAKE_CURRENT_BINARY_DIR}/lib/cmake/Slint/SlintConfig.cmake" INSTALL_DESTINATION lib/cmake/Slint)

write_basic_package_version_file(
  ${CMAKE_CURRENT_BINARY_DIR}/lib/cmake/Slint/SlintConfigVersion.cmake
  VERSION 0.2.5
  COMPATIBILITY SameMinorVersion
)

install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/lib/cmake/Slint/SlintConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/lib/cmake/Slint/SlintConfigVersion.cmake"
    DESTINATION lib/cmake/Slint
)

if (SLINT_FEATURE_COMPILER)
    install(FILES
        "${CMAKE_CURRENT_LIST_DIR}/cmake/SlintMacro.cmake"
        DESTINATION lib/cmake/Slint
    )
endif()

option(SLINT_PACKAGE_BUNDLE_QT "Internal setting to install Qt binary in the packages" OFF)
if(SLINT_PACKAGE_BUNDLE_QT)
    if(WIN32)
        find_package(Qt6 6.0 COMPONENTS Core Gui Widgets Svg)
        install(
            FILES
                $<TARGET_FILE:Qt6::Core>
                $<TARGET_FILE:Qt6::Gui>
                $<TARGET_FILE:Qt6::Widgets>
                $<TARGET_FILE:Qt6::Svg>
            TYPE LIB)

        install(
            FILES ${Qt6_DIR}/../../../plugins/platforms/qwindows.dll
            DESTINATION plugins/platforms)
        install(
            FILES ${Qt6_DIR}/../../../plugins/styles/qwindowsvistastyle.dll
            DESTINATION plugins/styles)
        install(
            FILES ${Qt6_DIR}/../../../plugins/imageformats/qsvg.dll
            DESTINATION plugins/imageformats)

        install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/../../licenses/ DESTINATION LICENSES)

        set(CMAKE_INSTALL_SYSTEM_RUNTIME_LIBS_SKIP TRUE)
        include(InstallRequiredSystemLibraries)
        install(FILES ${CMAKE_INSTALL_SYSTEM_RUNTIME_LIBS} TYPE LIB)
    endif()
endif(SLINT_PACKAGE_BUNDLE_QT)

set(CPACK_PACKAGE_NAME "Slint-cpp")
set(CPACK_PACKAGE_VENDOR "Slint")
set(CPACK_VERBATIM_VARIABLES true)
set(CPACK_PACKAGE_VERSION_MAJOR 0)
set(CPACK_PACKAGE_VERSION_MINOR 2)
set(CPACK_PACKAGE_VERSION_PATCH 5)
set(CPACK_PACKAGE_HOMEPAGE_URL "https://slint-ui.com")
set(CPACK_RESOURCE_FILE_LICENSE "${CMAKE_CURRENT_LIST_DIR}/../../LICENSE.md")
set(CPACK_RESOURCE_FILE_README "${CMAKE_CURRENT_LIST_DIR}/README.md")
set(CPACK_STRIP_FILES ON)
set(CPACK_NSIS_DEFINES "ManifestDPIAware true")
if(NOT WIN32)
   set(CPACK_GENERATOR "TGZ")
   set(CPACK_SYSTEM_NAME "${CMAKE_SYSTEM_NAME}-${CMAKE_SYSTEM_PROCESSOR}")
endif(NOT WIN32)
include(CPack)

if(BUILD_TESTING)
    FetchContent_Declare(
        Catch2
        GIT_REPOSITORY https://github.com/catchorg/Catch2.git
        GIT_TAG        v2.13.8
    )

    FetchContent_MakeAvailable(Catch2)

    find_package(Threads REQUIRED)

    macro(slint_test NAME)
        add_executable(test_${NAME} tests/${NAME}.cpp)
        target_link_libraries(test_${NAME} PRIVATE Slint Catch2::Catch2)
        target_compile_definitions(test_${NAME} PRIVATE
            SOURCE_DIR=\"${CMAKE_CURRENT_SOURCE_DIR}/\"
        )
        add_test(test_${NAME} ${CMAKE_CURRENT_BINARY_DIR}/test_${NAME})
        # Somehow the wrong relative rpath ends up in the binary, requiring us to change the
        # working directory.
        if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
            set_property(TEST test_${NAME} PROPERTY WORKING_DIRECTORY "${CMAKE_BINARY_DIR}")
        endif()
        if(MSVC)
            target_compile_options(test_${NAME} PRIVATE /W3)
        else()
            target_compile_options(test_${NAME} PRIVATE -Wall -Wextra -Werror)
        endif()
        if (CMAKE_CXX_COMPILER_ID STREQUAL GNU)
            # that warning has false positives
            target_compile_options(test_${NAME} PRIVATE -Wno-maybe-uninitialized)
        endif()

    endmacro(slint_test)
    slint_test(datastructures)
    if(SLINT_FEATURE_INTERPRETER)
        slint_test(interpreter)
    endif()
    slint_test(properties)
    slint_test(eventloop)
    target_link_libraries(test_eventloop PRIVATE Threads::Threads)
endif()
