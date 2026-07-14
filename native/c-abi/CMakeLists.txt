add_library(stitch_c_abi INTERFACE)

target_include_directories(stitch_c_abi INTERFACE "${CMAKE_CURRENT_SOURCE_DIR}/include")
target_compile_features(stitch_c_abi INTERFACE cxx_std_20)
