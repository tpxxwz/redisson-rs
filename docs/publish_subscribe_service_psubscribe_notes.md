# PublishSubscribeService.psubscribe 逐句分析

对应 Java 文件：`org.redisson.pubsub.PublishSubscribeService`

---

## 一、`psubscribe`（公开入口）

```java
public CompletableFuture<Collection<PubSubConnectionEntry>> psubscribe(
        ChannelName channelName, Codec codec, RedisPubSubListener<?>... listeners) {
```

> 公开入口，返回 `Collection<PubSubConnectionEntry>` 而不是单个，
> 是因为集群模式下同一 pattern 需要订阅到多个节点，每个节点对应一个 entry。

---

### 分支一：集群 + keyspace channel（多节点广播订阅）

```java
    if (isMultiEntity(channelName)) {
```

> `isMultiEntity` = 非单机模式 && channel 是 keyspace/keyevent 类型（`__keyspace@*__:` 或 `__keyevent@*__:`）。
> 这类 channel 每个节点都会产生事件，必须对所有节点都订阅。

```java
        Collection<MasterSlaveEntry> entrySet = connectionManager.getEntrySet();
        AtomicInteger statusCounter = new AtomicInteger(entrySet.size());
```

> 拿到所有节点集合，用原子计数器记录"还有多少个节点未确认订阅成功"。
> 目的是让 status 回调只触发一次（所有节点都确认后才通知上层）。

```java
        RedisPubSubListener[] ls = Arrays.stream(listeners).map(l -> {
            if (l instanceof PubSubPatternStatusListener) {
                return new PubSubPatternStatusListener((PubSubPatternStatusListener) l) {
                    @Override
                    public void onStatus(PubSubType type, CharSequence channel) {
                        if (statusCounter.get() == 0 || statusCounter.decrementAndGet() == 0) {
                            super.onStatus(type, channel);
                        }
                    }
                };
            }
            return l;
        }).toArray(RedisPubSubListener[]::new);
```

> 对每个 `PubSubPatternStatusListener` 类型的 listener 做包装：
> 覆写 `onStatus`，只有当计数器减到 0（所有节点都回调了）时才真正触发原始 listener 的 `onStatus`。
> 非 status listener 直接透传，不包装。
> 这是典型的"多路合并为一路通知"模式。

```java
        List<CompletableFuture<PubSubConnectionEntry>> futures = new ArrayList<>();
        for (MasterSlaveEntry entry : entrySet) {
            CompletableFuture<PubSubConnectionEntry> future =
                    subscribe(PubSubType.PSUBSCRIBE, codec, ChannelName.newList(channelName), entry, entry.getEntry(), ls);
            futures.add(future);
        }
```

> 对每个节点各自发起一次订阅，并行进行，收集所有 future。
> `entry.getEntry()` 传入具体的 `ClientConnectionsEntry`，用于绑定到指定连接。

```java
        CompletableFuture<Void> future = CompletableFuture.allOf(futures.toArray(new CompletableFuture[0]));
        return future.thenApply(r -> {
            return futures.stream().map(v -> v.getNow(null)).collect(Collectors.toList());
        });
    }
```

> 等所有节点订阅都完成（`allOf`），再把每个节点的 `PubSubConnectionEntry` 汇集成 `Collection` 返回。

---

### 分支二：单节点（正常路径）

```java
    MasterSlaveEntry entry = getEntry(channelName);
    if (entry == null) {
        int slot = connectionManager.calcSlot(channelName.getName());
        return connectionManager.getServiceManager().createNodeNotFoundFuture(channelName.toString(), slot);
    }
```

> 根据 channel name 找到对应的 `MasterSlaveEntry`（集群下根据 slot，单机就是唯一节点）。
> 找不到节点则直接返回一个已失败的 future，不进行后续操作。

```java
    CompletableFuture<PubSubConnectionEntry> f =
            subscribe(PubSubType.PSUBSCRIBE, codec, ChannelName.newList(channelName), entry, null, listeners);
    return f.thenApply(res -> Collections.singletonList(res));
}
```

> 单节点订阅，`clientEntry` 传 null 表示不绑定特定连接，由连接池自动选择。
> 结果包装成单元素 list 返回，和多节点路径保持返回类型一致。

---

## 二、`subscribeNoTimeout`（核心调度逻辑）

```java
private void subscribeNoTimeout(Codec codec, List<ChannelName> channelNames, MasterSlaveEntry entry,
        ClientConnectionsEntry clientEntry, CompletableFuture<PubSubConnectionEntry> promise,
        PubSubType type, AsyncSemaphore lock, AtomicInteger attempts, RedisPubSubListener<?>... listeners) {
```

> 不带超时的订阅核心方法，由 `subscribe`（带超时包装的那一层）调用。
> `promise` 是外层传进来的，最终完成时通知调用方。
> `lock` 是 channel 级别的 `AsyncSemaphore`，整个流程结束前不释放。

---

### 步骤一：尝试复用已有连接上的 entry

```java
    CompletableFuture<Boolean> future = addListeners(channelNames, entry, clientEntry, type, listeners, null, () -> {}, promise, lock);
    future.thenAccept(r1 -> {
        if (r1) {
            return; // true 表示 channel 已存在订阅，listeners 直接挂上去了，流程结束
        }
```

> 先调 `addListeners` 检查 channel 是否已经有现成的 `PubSubConnectionEntry`。
> 如果有（`r1 == true`），直接把 listeners 加到已有 entry 上，不需要新建连接，直接返回。
> 如果没有（`r1 == false`），继续往下走，需要找一个连接来承载这次订阅。

---

### 步骤二：从空闲连接池找可用连接

```java
        freePubSubLock.acquire().thenAccept(c -> {
```

> 拿全局的 `freePubSubLock`，保护对空闲连接池的并发访问，防止多个订阅同时抢占同一个空闲连接。

```java
            if (promise.isDone()) {
                lock.release();
                freePubSubLock.release();
                return;
            }
```

> double-check：如果 promise 已完成（可能超时或被取消），提前释放锁退出，避免无效操作。

```java
            PubSubEntry freePubSubConnections = entry2PubSubConnection.getOrDefault(entry, new PubSubEntry());
            PubSubConnectionEntry freeEntry = freePubSubConnections.getEntries().peek();
```

> 从当前节点（`entry`）的空闲 pubsub 连接池里取队头连接（`peek` 不移除）。
> 每条 pubsub 连接有订阅数上限（`subscriptionsPerConnection`），未满的连接放在这个池里。

```java
            if (freeEntry != null && clientEntry != null) {
                if (!clientEntry.getClient().equals(freeEntry.getConnection().getRedisClient())) {
                    freeEntry = null;
                }
            }
```

> 如果调用方指定了 `clientEntry`（要求绑定特定客户端连接），
> 则检查空闲 entry 是否属于同一个 Redis client，不匹配则视为无可用空闲连接。

---

### 步骤三：无空闲连接，新建连接

```java
            if (freeEntry == null) {
                freePubSubLock.release();
                connect(codec, channelNames, entry, clientEntry, promise, type, lock, attempts, listeners);
                return;
            }
```

> 没有可用的空闲连接，释放 `freePubSubLock`，调 `connect` 新建一条 pubsub 专用连接。

---

### 步骤四：有空闲连接，尝试占用

```java
            int remainFreeAmount = freeEntry.tryAcquire();
            if (remainFreeAmount == -1) {
                throw new IllegalStateException();
            }
```

> `tryAcquire` 原子地将该连接的剩余可用订阅槽位减 1，返回减后的剩余数。
> 返回 -1 表示已满，不应出现（上面 peek 出来说明有空位），所以视为非法状态。

```java
            PubSubConnectionEntry fe = freeEntry;
            CompletableFuture<Boolean> listenersFuture = addListeners(channelNames, entry, clientEntry, type, listeners,
                    freeEntry, () -> {
                        fe.release();
                        freePubSubLock.release();
                    }, promise, lock);
```

> 再次调 `addListeners`，这次传入 `freeEntry`，将 channel → freeEntry 的映射注册进去。
> releaser 回调：如果 channel 已被其他线程抢先注册（并发竞争），则释放刚才占用的槽位并放锁。

```java
            listenersFuture.thenAccept(r2 -> {
                if (r2) {
                    return; // 被其他线程抢先注册了，本次操作作废
                }
```

> `r2 == true` 表示 channel 已被别的线程注册，本次 `addListeners` 什么也没做，直接退出。

```java
                for (ChannelName channelName : channelNames) {
                    Collection<PubSubConnectionEntry> coll = name2entry.computeIfAbsent(
                            channelName, k -> Collections.newSetFromMap(new ConcurrentHashMap<>()));
                    coll.add(fe);
                }
```

> 注册成功，把 channelName → freeEntry 的映射写入 `name2entry`，
> 后续其他订阅者可以通过这个 map 找到现有 entry 来复用。

```java
                if (remainFreeAmount == 0) {
                    freePubSubConnections.getEntries().poll();
                }
                freePubSubLock.release();
```

> 如果这次占用后该连接已满（`remainFreeAmount == 0`），
> 把它从空闲池队列里移除（`poll`），不再分配给新的订阅。
> 然后释放 `freePubSubLock`。

```java
                fe.subscribe(codec, channelNames, promise, type, lock, listeners);
            });
        });
    });
}
```

> 最终在选定的连接上发送真正的 `PSUBSCRIBE` 命令，
> 命令成功后 `promise` 完成，`lock`（channel 级 semaphore）释放，通知外层调用方。

---

---

## 三、完整类结构总览

---

### 字段与内部类

```java
// 内部 key：(channelName, MasterSlaveEntry) 联合索引，定位某个节点上某个 channel 的连接
public static class PubSubKey { ChannelName channelName; MasterSlaveEntry entry; }

// 某个 MasterSlaveEntry 上所有空闲 pubsub 连接的队列
public static class PubSubEntry { Queue<PubSubConnectionEntry> entries; }
```

```java
private final AsyncSemaphore[] locks = new AsyncSemaphore[50];
// 50 个 channel 级别的 semaphore，按 channelName.hashCode() % 50 分片
// 防止同一 channel 并发重复订阅/取消

private final AsyncSemaphore freePubSubLock = new AsyncSemaphore(1);
// 全局唯一锁，保护对空闲连接池（entry2PubSubConnection）的并发访问

private final Map<ChannelName, Collection<PubSubConnectionEntry>> name2entry;
// channel → 持有该 channel 订阅的所有 PubSubConnectionEntry（集合）
// 集群 keyspace 场景下同一 channel 会有多个 entry（每个节点一个）

private final ConcurrentMap<PubSubKey, PubSubConnectionEntry> name2PubSubConnection;
// (channelName, MasterSlaveEntry) → PubSubConnectionEntry
// 用于快速判断"某个节点上是否已经订阅了某个 channel"

private final ConcurrentMap<MasterSlaveEntry, PubSubEntry> entry2PubSubConnection;
// MasterSlaveEntry → 该节点上所有还有空余槽位的 pubsub 连接队列
// 新的订阅先从这里找空闲连接，找不到再 connect()

private final Map<Tuple<ChannelName, ClientConnectionsEntry>, PubSubConnectionEntry> key2connection;
// (channelName, ClientConnectionsEntry) → entry，用于 client tracking 场景
// 需要把订阅绑定到特定的底层连接（CLIENT TRACKING ON REDIRECT <id>）

private final SemaphorePubSub semaphorePubSub;    // 分布式信号量专用 pub/sub 处理器
private final CountDownLatchPubSub countDownLatchPubSub;  // 分布式 CountDownLatch 专用
private final LockPubSub lockPubSub;              // 分布式锁专用 pub/sub 处理器

private final Set<PubSubConnectionEntry> trackedEntries;
// 已经对其发过 CLIENT TRACKING ON REDIRECT 的 entry 集合，避免重复发

private boolean shardingSupported = false;  // 是否支持 SSUBSCRIBE（Redis 7.0+ 集群 sharded）
private boolean patternSupported = true;    // 是否支持 PSUBSCRIBE（极少数场景会被禁用）
```

---

### `isMultiEntity(channelName)`

```java
public boolean isMultiEntity(ChannelName channelName) {
    return !connectionManager.getServiceManager().getCfg().isSingleConfig()
            && channelName.isKeyspace();
}
```

- `isSingleConfig()` = 单机/Unix socket 模式
- `isKeyspace()` = channel 以 `__keyspace` 或 `__keyevent` 开头
- 两者同时满足才是 multiEntity：非单机 && keyspace channel
- 含义：keyspace 通知在集群中是各节点本地发出的，必须对所有节点都订阅才能收全

---

### `subscribe(codec, channelName/channelNames, listeners)`（公开 SUBSCRIBE 入口）

和 `psubscribe` 结构完全对称：

- `isMultiEntity` 时对所有节点并行订阅，status 回调合并（`AtomicInteger` 计数）
- 否则按 slot 找节点，调内部 `subscribe(PubSubType.SUBSCRIBE, ...)`
- 支持传 `List<ChannelName>` 一次批量订阅多个 channel（底层一次 subscribe 命令）

---

### `ssubscribe(codec, channelNames, listeners)`（公开 SSUBSCRIBE 入口）

```java
public CompletableFuture<PubSubConnectionEntry> ssubscribe(...) {
    MasterSlaveEntry entry = getEntry(channelNames.get(0));
    return subscribe(PubSubType.SSUBSCRIBE, codec, channelNames, entry, null, listeners);
}
```

- Sharded pubsub（Redis 7.0+），按 key 的 slot 路由到对应节点
- 不需要广播到所有节点，slot 决定了事件在哪个节点上发出
- `shardingSupported` 为 true 时，普通 `subscribe` 也会自动走 SSUBSCRIBE

---

### `subscribe(type, codec, channelNames, entry, clientEntry, listeners)`（私有，带超时包装）

```java
private CompletableFuture<PubSubConnectionEntry> subscribe(...) {
    CompletableFuture<PubSubConnectionEntry> promise = new CompletableFuture<>();
    Tuple<AsyncSemaphore, Set<AsyncSemaphore>> locks = acquire(channelNames);
    // acquire() 对 channelNames 中所有 channel 各自加锁，汇聚成一个组合 semaphore

    // 启动超时定时器：subscriptionTimeout ms 内未完成则 promise 异常结束
    Timeout lockTimeout = ...newTimeout(t -> promise.completeExceptionally(...), timeout);

    lock.acquire().thenAccept(r -> {
        if (!lockTimeout.cancel() || promise.isDone()) { lock.release(); return; }
        subscribeNoTimeout(codec, channelNames, entry, clientEntry, promise, type, lock, ...);
        timeout(promise, newTimeout); // 再加一个订阅完成超时
    });

    // 第二次 acquire 等待 promise 完成后统一释放所有 channel 锁
    lock.acquire().thenAccept(rr -> locks.getT2().forEach(l -> l.release()));
    return promise;
}
```

关键点：`acquire(channelNames)` 返回一个组合 semaphore：
- 内部对每个 channelName 各取一把锁（去重），并行等待全部获取
- 全部获取后释放组合 semaphore，触发 `lock.acquire()` 继续

---

### `acquire(channelNames)`（多 channel 批量加锁）

```java
private Tuple<AsyncSemaphore, Set<AsyncSemaphore>> acquire(List<ChannelName> channelNames) {
    // 收集所有不重复的 channel 锁
    Set<AsyncSemaphore> locks = channelNames.stream()
        .map(this::getSemaphore).collect(toSet());
    // 并行 acquire 全部，都完成后释放组合 result semaphore
    CompletableFuture.allOf(locks.stream().map(AsyncSemaphore::acquire)...)
        .thenAccept(r -> result.release());
    return new Tuple<>(result, locks);
}
```

---

### `addListeners(...)`（复用已有 entry 的核心逻辑）

```java
private CompletableFuture<Boolean> addListeners(
        List<ChannelName> channelNames, MasterSlaveEntry entry,
        ClientConnectionsEntry clientEntry, PubSubType type,
        RedisPubSubListener<?>[] listeners,
        PubSubConnectionEntry freeEntry,   // null = 只查，非null = 尝试注册
        Runnable releaser,                 // 竞争失败时的回滚回调
        CompletableFuture<PubSubConnectionEntry> promise,
        AsyncSemaphore lock)
```

**两次调用，语义不同：**

1. `freeEntry = null`：只查 `name2PubSubConnection` / `key2connection`，看 channel 是否已有 entry
   - 已有 → 把 listeners 挂上去，返回 `true`（不需要新建连接）
   - 没有 → 返回 `false`（继续走新建连接流程）

2. `freeEntry != null`：尝试用 CAS（`putIfAbsent`）把 freeEntry 注册进 `name2PubSubConnection`
   - 注册成功 → 返回 `false`（继续走 subscribe 命令）
   - 被其他线程抢先 → `releaser.run()` 回滚，返回 `true`（作废本次操作）

---

### `connect(...)`（新建 pubsub 连接）

```java
private void connect(...) {
    // 从连接池申请一条新的 pubsub 专用连接
    CompletableFuture<RedisPubSubConnection> connFuture = msEntry.nextPubSubConnection(clientEntry);

    // 超时：连接获取失败则走 trySubscribe 重试
    newTimeout(t -> { if (!connFuture.cancel(false)...) trySubscribe(...); }, retryDelay);

    connFuture.thenAccept(conn -> {
        freePubSubLock.acquire().thenAccept(c -> {
            PubSubConnectionEntry entry = new PubSubConnectionEntry(conn, ...);
            // 同样调 addListeners 做 CAS 注册，防止并发重复
            // 注册成功后：
            //   1. 把 entry 加入 name2entry
            //   2. 如果 entry 还有空余槽位，加入 entry2PubSubConnection 空闲池
            //   3. 调 entry.subscribe() 真正发送 Redis 命令
        });
    });
}
```

---

### `trySubscribe(...)`（重试调度）

```java
private void trySubscribe(...) {
    if (attempts.get() == config.getRetryAttempts()) {
        promise.completeExceptionally(new RedisTimeoutException(...));
        return;
    }
    attempts.incrementAndGet();
    MasterSlaveEntry entry = getEntry(channelName);
    if (entry == null) {
        // 节点尚未发现，延迟后重试
        newTimeout(tt -> trySubscribe(...), retryDelay);
        return;
    }
    subscribeNoTimeout(...);
}
```

用于节点暂时不可用时的退避重试，最多 `retryAttempts` 次。

---

### `unsubscribeLocked(topicType, channelName, ce)`（有锁取消订阅）

```java
CompletableFuture<Void> unsubscribeLocked(PubSubType topicType, ChannelName channelName, PubSubConnectionEntry ce) {
    remove(channelName, ce);      // 先从内存移除所有索引

    // 注册临时 onStatus 监听器，等待 Redis 回包确认
    ce.unsubscribe(topicType, channelName, listener);  // 发送 UNSUBSCRIBE/PUNSUBSCRIBE
    // Redis 回包 → onStatus → freePubSubLock → release(ce) → result.complete(null)
    return result;
}
```

先摘内存，再等 Redis 确认，确认后 `release(ce)` 把连接槽位归还空闲池。

---

### `remove(channelName, entry)`（内存索引清理）

同时清理四个数据结构：
1. `name2PubSubConnection.remove(PubSubKey(channelName, entry.getEntry()))`
2. `key2connection.remove(Tuple(channelName, clientConnectionsEntry))`
3. `trackedEntries.remove(entry)`（如果 tracking 引用计数降到 0）
4. `name2entry` 中移除该 entry，如果 set 变空则整条记录删除

---

### `release(entry)`（连接槽位归还）

```java
private void release(PubSubConnectionEntry entry) {
    entry.release();         // 槽位计数 +1
    if (entry.isFree()) {    // 连接的所有槽位都空了
        entry2PubSubConnection 中移除该 entry;
        msEntry.returnPubSubConnection(conn);  // 连接还给连接池
        return;
    }
    // 还有槽位剩余：把 entry 放回 entry2PubSubConnection 空闲队列
    // 注意：如果连接已关闭则不放回
}
```

---

### `unsubscribe(channelName, entry, topicType)`（公开取消订阅，带 semaphore）

```java
CompletableFuture<Codec> unsubscribe(...) {
    AsyncSemaphore lock = getSemaphore(channelName);
    return lock.acquire().thenCompose(v -> {
        // 读取该 channel 在这条连接上的 codec（用于返回给调用方）
        Codec entryCodec = entry.getConnection().getPatternChannels().get(channelName); // PUNSUBSCRIBE
        // 或 getChannels() / getShardedChannels() 根据 topicType

        return unsubscribeLocked(topicType, channelName, entry)
            .whenComplete((r, e) -> lock.release())
            .thenApply(r -> entryCodec);
    });
}
```

返回 `Codec` 是为了 `reattachPubSubListeners` 重订阅时能用原来的 codec。

---

### `reattachPubSub(RedisPubSubConnection)`（断线重订阅）

```java
public void reattachPubSub(RedisPubSubConnection redisPubSubConnection) {
    MasterSlaveEntry en = connectionManager.getEntry(redisPubSubConnection.getRedisClient());
    reattachPubSubListeners(conn.getChannels().keySet(),        en, UNSUBSCRIBE);
    reattachPubSubListeners(conn.getShardedChannels().keySet(), en, SUNSUBSCRIBE);
    reattachPubSubListeners(conn.getPatternChannels().keySet(), en, PUNSUBSCRIBE);
}
```

连接断开时由上层调用，对该连接上所有已订阅的 channel 执行"取消 + 重订阅"。

```java
private void reattachPubSubListeners(Set<ChannelName> channels, MasterSlaveEntry en, PubSubType topicType) {
    for (ChannelName channelName : channels) {
        // 1. 取消订阅（拿回 codec）
        CompletableFuture<Codec> subscribeCodecFuture = unsubscribe(channelName, entry, topicType);
        // 2. 用原 codec 和 listeners 重新订阅（1秒后重试直到成功）
        subscribeCodecFuture.whenComplete((codec, e) -> {
            if (topicType == PUNSUBSCRIBE) psubscribe(en, channelName, listeners, codec);
            else if (topicType == SUNSUBSCRIBE) ssubscribe(channelName, listeners, codec);
            else subscribe(channelName, listeners, codec);
        });
    }
}
```

私有 `psubscribe(MasterSlaveEntry oldEntry, ...)` 的逻辑：
- multiEntity 场景下，找一个**还没有**该 channel 订阅的节点（排除旧节点）
- 找不到则 1 秒后重试

---

### `removeListenerAsync(type, channelNames, listener/ids)`（移除单个 listener）

```java
// 两个重载：按 EventListener 对象 或 按 listener id（System.identityHashCode）
public CompletableFuture<Void> removeListenerAsync(PubSubType type, List<ChannelName> channelNames, EventListener listener)
public CompletableFuture<Void> removeListenerAsync(PubSubType type, List<ChannelName> channelNames, Integer... listenerIds)
```

内部逻辑：
1. 过滤掉 `name2entry` 中不存在的 channel
2. `acquire(channelNames)` 批量加锁 + 超时
3. 对每个 channel 的每个 entry 调 `consumer`（即 `entry.removeListener(...)`）
4. 如果 entry 上该 channel 已无 listener：调 `unsubscribeLocked` 真正取消
5. 否则只是移除 listener，不发 Redis 命令

---

### `removeAllListenersAsync(type, channelNames...)`（移除所有 listener）

逻辑与 `removeListenerAsync` 类似，区别：
- 不走 `acquire()` 组合锁，直接用单个 `getSemaphore(channelName)` 加锁
- 不调 `entry.removeListener`，而是判断 `entry.hasListeners(channelName)` 后直接 `unsubscribeLocked`
- 本质是：只要该 channel 还有 listener，就直接取消整个订阅

---

### Client Tracking 相关订阅

```java
// 订阅 FlushListener（监听 CLIENT NO-EVICT / FLUSH 事件）
public CompletableFuture<Integer> subscribe(CommandAsyncExecutor, FlushListener)
// 订阅 TrackingListener（监听 CLIENT TRACKING invalidation）
public CompletableFuture<Integer> subscribe(CommandAsyncExecutor, TrackingListener)
// 订阅特定 key 的 tracking（CLIENT TRACKING ON REDIRECT id 需指定 slot 节点）
public CompletableFuture<Integer> subscribe(String key, Codec, CommandAsyncExecutor, TrackingListener)
```

这三个方法都订阅的是 `ChannelName.TRACKING`（即 `__redis__:invalidate`），区别在于：
- 无 key：对所有节点都订阅，`CLIENT TRACKING ON REDIRECT <id>` 发到每个节点
- 有 key：只订阅该 key 所在节点的连接

`registerClientTrackingListener` 是公共收尾步骤：
1. 收集所有新加入的 entry（未在 `trackedEntries` 中的）
2. 对每个新 entry 的连接执行 `CLIENT ID` 拿到连接 ID
3. 发送 `CLIENT TRACKING ON REDIRECT <id>`，让 Redis server 把 invalidation 通知推到该连接

---

### `checkPatternSupport` / `checkShardingSupport`

```java
public void checkPatternSupport(RedisConnection connection) {
    // 发 PUBSUB NUMPAT 试探，失败则 patternSupported = false
}
public void checkShardingSupport(ShardedSubscriptionMode mode, RedisConnection connection) {
    // AUTO 模式：发 PUBSUB SHARDNUMSUB 0 试探，成功则 shardingSupported = true
    // ON 模式：直接 shardingSupported = true
}
```

启动时由连接管理器调用，探测 Redis server 版本能力。
`shardingSupported = true` 后，`subscribe` 自动走 SSUBSCRIBE，`getPublishCommand()` 返回 `SPUBLISH`。

---

### `getPublishCommand()`

```java
public String getPublishCommand() {
    if (shardingSupported) return "SPUBLISH";
    return "PUBLISH";
}
```

上层调用 `RTopic.publish()` 时通过此方法决定发 `PUBLISH` 还是 `SPUBLISH`。

---

## 四、Rust 侧对应关系总结

| Java 逻辑 | Rust / fred 处理 |
|---|---|
| 多节点广播订阅 `isMultiEntity` | fred 集群模式内部处理，无需手动 |
| channel 级 `AsyncSemaphore` | `PublishSubscribeService.semaphores`（已有） |
| `name2PubSubConnection` 复用已有连接 | `pattern_listeners.contains_key` 判断是否已订阅 |
| 空闲连接池 / 新建连接 | fred `SubscriberClient` 内部管理，无需手动 |
| 断线重订阅 | fred `manage_subscriptions` 自动处理 |
| listener 注册与消息分发 | `pattern_listeners` 存储，需手动启动分发 task |
