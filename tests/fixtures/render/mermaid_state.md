```mermaid
stateDiagram-v2
  [*] --> Idle
  Idle --> Busy : go
  Busy --> [*]
  Busy : working
```

```mermaid
stateDiagram-v2
  state Outer {
    A --> B
  }
```
