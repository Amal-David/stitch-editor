qt_add_executable(stitch_editor_shell
  src/main.cpp
)

target_link_libraries(stitch_editor_shell PRIVATE Qt6::Quick stitch_c_abi)

if(APPLE)
  target_compile_definitions(stitch_editor_shell PRIVATE STITCH_EXPECT_METAL=1)
elseif(WIN32)
  target_compile_definitions(stitch_editor_shell PRIVATE STITCH_EXPECT_D3D11=1)
endif()
