#include <jni.h>
#include <string>
#include "muse_presence.h"

static std::string str(JNIEnv* env, jstring s) {
    if (!s) return "";
    const char* c = env->GetStringUTFChars(s, nullptr);
    std::string r = c ? c : "";
    if (c) env->ReleaseStringUTFChars(s, c);
    return r;
}

extern "C" {
JNIEXPORT jint JNICALL Java_vn_muse_app_MuseSdk_nStart(JNIEnv* env, jclass, jlong appId, jstring token) {
    return muse_pres_start((uint64_t)appId, str(env, token).c_str());
}
JNIEXPORT void JNICALL Java_vn_muse_app_MuseSdk_nUpdate(JNIEnv* env, jclass, jstring title, jstring artist, jstring image, jlong startS, jlong endS) {
    muse_pres_update(str(env, title).c_str(), str(env, artist).c_str(), str(env, image).c_str(), (int64_t)startS, (int64_t)endS);
}
JNIEXPORT void JNICALL Java_vn_muse_app_MuseSdk_nClear(JNIEnv*, jclass) { muse_pres_clear(); }
JNIEXPORT jint JNICALL Java_vn_muse_app_MuseSdk_nStatus(JNIEnv*, jclass) { return muse_pres_status(); }
JNIEXPORT jstring JNICALL Java_vn_muse_app_MuseSdk_nError(JNIEnv* env, jclass) { return env->NewStringUTF(muse_pres_error()); }
}
