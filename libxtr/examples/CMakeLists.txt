add_executable(client-demo client-demo.cpp)

target_include_directories(client-demo
    PRIVATE ${CMAKE_SOURCE_DIR}/include
)

target_link_libraries(client-demo
    -L${CMAKE_SOURCE_DIR}/../target/debug
    -lxtr
)
