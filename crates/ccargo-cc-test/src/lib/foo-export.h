// This file has been generated automatically, please do not edit.
#ifndef FOO_EXPORT_H
#define FOO_EXPORT_H
#ifdef FOO_STATIC
    #define FOO_EXPORT
#else
    #ifdef _MSC_VER
        #ifdef FOO_EXPORTS
            #define FOO_EXPORT __declspec(dllexport)
        #else
            #define FOO_EXPORT __declspec(dllimport)
        #endif // FOO_EXPORTS
    #elif defined(__clang__) || defined(__GNUC__)
        #define FOO_EXPORT __attribute__((visibility("default")))
    #else
        #define FOO_EXPORT
    #endif
#endif // FOO_STATIC
#endif // FOO_EXPORT_H
