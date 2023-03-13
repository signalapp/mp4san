#include <stdio.h>
#include <string.h>
#include "log.h"

void mp4san_test_ffmpeg_log(int, const char*);

void mp4san_test_ffmpeg_log_callback(void* ptr, int level, const char* fmt, va_list vl) {
    (void) ptr;
  
    char buffer[4096];
    memset(buffer, 0, sizeof(buffer));
    vsnprintf(buffer, sizeof(buffer), fmt, vl);
    mp4san_test_ffmpeg_log(level, buffer);
}
