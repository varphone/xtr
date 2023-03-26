add_executable(client-demo client-demo.cpp)

target_link_libraries(client-demo
    xtr::xtr
)

if(BUILD_SHARED_LIBS)
    add_custom_command(TARGET client-demo
        POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy $<TARGET_RUNTIME_DLLS:client-demo> $<TARGET_FILE_DIR:client-demo> COMMAND_EXPAND_LISTS
    )
endif()
