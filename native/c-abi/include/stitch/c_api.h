#ifndef STITCH_C_API_H
#define STITCH_C_API_H

#include <stdint.h>

/* T-0015 layout-only contract. T-0009 owns callable ABI implementation. */

#ifdef __cplusplus
extern "C" {
#endif

typedef struct stitch_core_handle stitch_core_handle;
typedef struct stitch_frame_lease stitch_frame_lease;

typedef uint64_t stitch_revision;
typedef uint64_t stitch_device_generation;

typedef enum stitch_result {
  STITCH_RESULT_OK = 0,
  STITCH_RESULT_STALE_DEVICE = 1,
  STITCH_RESULT_STALE_REVISION = 2,
  STITCH_RESULT_UNAVAILABLE = 3,
  STITCH_RESULT_INVALID_ARGUMENT = 4,
} stitch_result;

typedef struct stitch_frame_metadata {
  uint32_t struct_size;
  stitch_revision revision;
  stitch_device_generation device_generation;
  int64_t presentation_time_numerator;
  int64_t presentation_time_denominator;
} stitch_frame_metadata;

/* Platform bridges retain and retire native objects; callers never receive a raw GPU pointer. */
stitch_result stitch_frame_metadata_get(const stitch_frame_lease *lease,
                                        stitch_frame_metadata *out_metadata);
void stitch_frame_lease_release(stitch_frame_lease *lease);

#ifdef __cplusplus
}
#endif

#endif
