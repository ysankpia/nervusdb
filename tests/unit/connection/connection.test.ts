import { describe, expect, it } from 'vitest';
import { buildConnectionUri, sanitizeConnectionOptions } from '@/index';

describe('连接字符串构建', () => {
  it('应根据默认端口与参数生成稳定的连接 URI', () => {
    const uri = buildConnectionUri({
      driver: 'postgresql',
      host: 'db.internal.local',
      username: 'analytics',
      password: 'super$secret',
      database: 'warehouse',
      parameters: {
        poolSize: 10,
        sslmode: 'require',
      },
    });

    expect(uri).toBe(
      'postgresql://analytics:super%24secret@db.internal.local:5432/warehouse?poolSize=10&sslmode=require',
    );
  });

  it('缺少关键字段时抛出明确错误', () => {
    expect(() =>
      buildConnectionUri({
        driver: 'mysql',
        host: 'localhost',
        username: 'root',
        password: '',
      }),
    ).toThrow(/缺少必要连接字段: password/);
  });
});

describe('敏感信息脱敏', () => {
  it('仅保留口令末尾四位', () => {
    const sanitized = sanitizeConnectionOptions({
      driver: 'postgresql',
      host: 'db.internal',
      username: 'etl',
      password: 'synapse-secret',
      database: 'warehouse',
    });

    expect(sanitized.password).toBe('**********cret');
    expect(sanitized.port).toBe(5432);
  });
});
