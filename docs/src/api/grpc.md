### Servicios gRPC

#### IxmatiWrite

```proto
service IxmatiWrite {
  rpc Write (WriteRequest) returns (WriteResponse);
}

message WriteRequest {
  string op = 1;
  string store = 2;
  string entity = 3;
  string key = 4;
  uint64 version = 5;
  string idempotency_key = 6;
  string ack_mode = 7;
  bytes payload = 8;
}
```

#### IxmatiRead

```proto
service IxmatiRead {
  rpc Read (ReadRequest) returns (ReadResponse);
}

message ReadRequest {
  string store = 1;
  string entity = 2;
  string key = 3;
  string projection = 4;
}
```

#### IxmatiStatus

```proto
service IxmatiStatus {
  rpc GetStatus (StatusRequest) returns (StatusResponse);
  rpc Health (HealthRequest) returns (HealthResponse);
}
```
