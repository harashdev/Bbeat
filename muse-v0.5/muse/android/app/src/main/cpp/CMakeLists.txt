cmake_minimum_required(VERSION 3.22)
project(muse_presence CXX)
set(CMAKE_CXX_STANDARD 17)

# DISCORD_SDK_DIR: thư mục "discord_social_sdk" đã giải nén; DISCORD_AAR_DIR: thư mục AAR đã giải nén (có jni/<abi>/libdiscord_partner_sdk.so)
add_library(discord_partner_sdk SHARED IMPORTED)
set_target_properties(discord_partner_sdk PROPERTIES
    IMPORTED_LOCATION ${DISCORD_AAR_DIR}/jni/${ANDROID_ABI}/libdiscord_partner_sdk.so)

add_library(muse_presence SHARED
    muse_jni.cpp
    ${MUSE_NATIVE_DIR}/muse_presence.cpp)
target_include_directories(muse_presence PRIVATE ${DISCORD_SDK_DIR}/include ${MUSE_NATIVE_DIR})
target_link_libraries(muse_presence discord_partner_sdk log)
