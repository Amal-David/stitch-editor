cmake_minimum_required(VERSION 3.31.6)

project(stitch_editor VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(APPLE)
  set(STITCH_REQUIRED_QT_BACKEND "Metal")
elseif(WIN32)
  set(STITCH_REQUIRED_QT_BACKEND "Direct3D11")
else()
  message(FATAL_ERROR "Stitch Editor supports macOS and Windows only; got ${CMAKE_SYSTEM_NAME}.")
endif()

if(NOT DEFINED Qt6_DIR AND NOT DEFINED ENV{Qt6_DIR})
  message(FATAL_ERROR
    "Qt 6.11.1 is required but Qt6_DIR is unset. This offline bootstrap never downloads Qt. "
    "Install the pinned Qt kit, then pass -DQt6_DIR=/absolute/path/to/Qt6 or set Qt6_DIR.")
endif()

find_package(Qt6 6.11.1 EXACT CONFIG REQUIRED COMPONENTS Quick)

add_subdirectory(native/c-abi)
add_subdirectory(desktop/qt)
