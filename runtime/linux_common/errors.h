// What the C library of a Linux system says for each error number, for
// the ones that reading and writing files run into. The numbers are the
// same on every processor architecture azoL supports. Returns null for a
// number that is not listed.
static const char *error_text(long error) {
    switch (error) {
    case 1: return "Operation not permitted";
    case 2: return "No such file or directory";
    case 4: return "Interrupted system call";
    case 5: return "Input/output error";
    case 6: return "No such device or address";
    case 9: return "Bad file descriptor";
    case 11: return "Resource temporarily unavailable";
    case 12: return "Cannot allocate memory";
    case 13: return "Permission denied";
    case 14: return "Bad address";
    case 16: return "Device or resource busy";
    case 17: return "File exists";
    case 19: return "No such device";
    case 20: return "Not a directory";
    case 21: return "Is a directory";
    case 22: return "Invalid argument";
    case 23: return "Too many open files in system";
    case 24: return "Too many open files";
    case 26: return "Text file busy";
    case 27: return "File too large";
    case 28: return "No space left on device";
    case 30: return "Read-only file system";
    case 32: return "Broken pipe";
    case 36: return "File name too long";
    case 40: return "Too many levels of symbolic links";
    case 122: return "Disk quota exceeded";
    default: return 0;
    }
}
