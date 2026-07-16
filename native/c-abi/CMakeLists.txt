add_library(stitch_c_abi INTERFACE)

target_include_directories(stitch_c_abi INTERFACE "${CMAKE_CURRENT_SOURCE_DIR}/include")
target_compile_features(stitch_c_abi INTERFACE cxx_std_20)

set(STITCH_C_ABI_CARGO_TARGET "${CMAKE_BINARY_DIR}/cargo")
if(WIN32)
  set(STITCH_C_ABI_LIBRARY "${STITCH_C_ABI_CARGO_TARGET}/debug/stitch_c_abi.dll")
  set(STITCH_C_ABI_IMPLIB "${STITCH_C_ABI_CARGO_TARGET}/debug/stitch_c_abi.dll.lib")
elseif(APPLE)
  set(STITCH_C_ABI_LIBRARY "${STITCH_C_ABI_CARGO_TARGET}/debug/libstitch_c_abi.dylib")
else()
  set(STITCH_C_ABI_LIBRARY "${STITCH_C_ABI_CARGO_TARGET}/debug/libstitch_c_abi.so")
endif()
set(STITCH_C_ABI_BYPRODUCTS "${STITCH_C_ABI_LIBRARY}")
if(WIN32)
  list(APPEND STITCH_C_ABI_BYPRODUCTS "${STITCH_C_ABI_IMPLIB}")
endif()
add_custom_target(stitch_c_abi_rust_build
  COMMAND ${CMAKE_COMMAND} -E env CARGO_TARGET_DIR=${STITCH_C_ABI_CARGO_TARGET} rustup run 1.97.0 cargo build --manifest-path ${CMAKE_CURRENT_SOURCE_DIR}/rust/Cargo.toml --offline
  BYPRODUCTS ${STITCH_C_ABI_BYPRODUCTS})
add_library(stitch_c_abi_rust SHARED IMPORTED GLOBAL)
set_target_properties(stitch_c_abi_rust PROPERTIES IMPORTED_LOCATION "${STITCH_C_ABI_LIBRARY}")
if(WIN32)
  set_target_properties(stitch_c_abi_rust PROPERTIES IMPORTED_IMPLIB "${STITCH_C_ABI_IMPLIB}")
endif()
add_dependencies(stitch_c_abi_rust stitch_c_abi_rust_build)
target_link_libraries(stitch_c_abi INTERFACE stitch_c_abi_rust)

include(CTest)
if(BUILD_TESTING)
  add_executable(stitch_c_abi_lifecycle_test tests/c_api_layout_test.cpp)
  target_link_libraries(stitch_c_abi_lifecycle_test PRIVATE stitch_c_abi stitch_c_abi_rust)
  add_dependencies(stitch_c_abi_lifecycle_test stitch_c_abi_rust_build)
  if(APPLE)
    set_target_properties(stitch_c_abi_lifecycle_test PROPERTIES BUILD_RPATH "${STITCH_C_ABI_CARGO_TARGET}/debug")
  elseif(WIN32)
    add_custom_command(TARGET stitch_c_abi_lifecycle_test POST_BUILD
      COMMAND ${CMAKE_COMMAND} -E copy_if_different
        "${STITCH_C_ABI_LIBRARY}"
        "$<TARGET_FILE_DIR:stitch_c_abi_lifecycle_test>")
  endif()
  add_test(NAME stitch_c_abi_lifecycle_test COMMAND stitch_c_abi_lifecycle_test)
endif()
