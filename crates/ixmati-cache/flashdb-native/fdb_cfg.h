// fdb_cfg.h — FlashDB configuration for Ixmati spike (Linux x86_64)
// Generated for TASK-WRITE-0001 / DEC-0009

#ifndef FDB_CFG_H
#define FDB_CFG_H

#define FDB_USING_KVDB
#define FDB_USING_TSDB
#define FDB_USING_FILE_LIBC_MODE
#define FDB_WRITE_GRAN         1
#define FDB_STRICT_ALIGN       1

// KVDB default config
#define FDB_SECTOR_CACHE_SIZE  4096
#define FDB_FILE_CACHE_NUM     8

// TSDB default config (time series)
#define FDB_TIME_STAMP_SIZE    8

// Logging: redirect to printf for spike
#define FDB_PRINT printf

#endif // FDB_CFG_H
