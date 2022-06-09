add_executable(client-demo client-demo.cpp)
add_executable(server-demo server-demo.cpp)

target_include_directories(client-demo
    PRIVATE ${CMAKE_SOURCE_DIR}/include
)

target_include_directories(server-demo
    PRIVATE ${CMAKE_SOURCE_DIR}/include
)

target_link_libraries(client-demo
    -L${CMAKE_SOURCE_DIR}/../target/release
    -lxtr
)

target_link_libraries(server-demo
    -L${CMAKE_SOURCE_DIR}/../target/release
    -lxtr
)
