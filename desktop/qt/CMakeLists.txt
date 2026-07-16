find_package(Qt6 6.11.1 EXACT CONFIG REQUIRED COMPONENTS QuickControls2)
qt_policy(SET QTP0001 NEW)
qt_policy(SET QTP0004 NEW)

if(APPLE)
  set(STITCH_SHELL_SOURCES
    src/editor_controller.h
    src/editor_controller.mm
    src/main.mm
    src/preview_item.h
    src/preview_item.mm
  )
else()
  # The Windows-owned D3D11 bridge replaces this guarded bootstrap in T-0011.
  set(STITCH_SHELL_SOURCES src/main.cpp)
endif()

qt_add_executable(stitch_editor_shell ${STITCH_SHELL_SOURCES})

set_target_properties(stitch_editor_shell PROPERTIES AUTOMOC ON)

qt_add_qml_module(stitch_editor_shell
  URI StitchShell
  VERSION 1.0
  QML_FILES src/Main.qml
)

target_link_libraries(stitch_editor_shell PRIVATE
  Qt6::Quick
  Qt6::QuickControls2
  stitch_c_abi
)
add_dependencies(stitch_editor_shell stitch_c_abi_rust_build)

if(APPLE)
  target_compile_definitions(stitch_editor_shell PRIVATE STITCH_EXPECT_METAL=1)
  find_library(STITCH_METAL_FRAMEWORK Metal REQUIRED)
  target_link_libraries(stitch_editor_shell PRIVATE ${STITCH_METAL_FRAMEWORK})
  get_target_property(STITCH_C_ABI_IMPORTED_LOCATION
    stitch_c_abi_rust IMPORTED_LOCATION)
  get_filename_component(STITCH_C_ABI_LIBRARY_DIR
    "${STITCH_C_ABI_IMPORTED_LOCATION}" DIRECTORY)
  set_target_properties(stitch_editor_shell PROPERTIES
    BUILD_RPATH "${STITCH_C_ABI_LIBRARY_DIR}"
  )
elseif(WIN32)
  target_compile_definitions(stitch_editor_shell PRIVATE STITCH_EXPECT_D3D11=1)
  get_target_property(STITCH_C_ABI_IMPORTED_LOCATION
    stitch_c_abi_rust IMPORTED_LOCATION)
  add_custom_command(TARGET stitch_editor_shell POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
      "${STITCH_C_ABI_IMPORTED_LOCATION}"
      "$<TARGET_FILE_DIR:stitch_editor_shell>")
endif()
