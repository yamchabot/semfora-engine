//! TypeScript file templates for incremental build benchmark
//!
//! 65 steps building an Event-driven API:
//! - Phase 1 (1-10): Foundation - types, config, utils
//! - Phase 2 (11-20): Repository Layer - data access
//! - Phase 3 (21-35): Service Layer - business logic
//! - Phase 4 (36-50): Handlers - request handling
//! - Phase 5 (51-65): Routes & Integration - wiring

use super::types::FileTemplate;

/// Get all file templates in order
pub fn get_templates() -> Vec<FileTemplate> {
    vec![
        // ============================================================
        // PHASE 1: Foundation (Steps 1-10)
        // ============================================================
        FileTemplate {
            step: 1,
            path: "src/types/common.ts",
            purpose: "Base type definitions",
            content: r#"// Common type definitions for the Event API

export interface Result<T, E = Error> {
  success: boolean;
  data?: T;
  error?: E;
}

export interface PaginatedResult<T> {
  items: T[];
  total: number;
  page: number;
  pageSize: number;
  hasMore: boolean;
}

export type EventStatus = 'pending' | 'processing' | 'completed' | 'failed';

export interface Timestamp {
  createdAt: Date;
  updatedAt: Date;
}
"#,
        },
        FileTemplate {
            step: 2,
            path: "src/types/events.ts",
            purpose: "Event type definitions",
            content: r#"// Event type definitions
import { EventStatus, Timestamp } from './common';

export interface EventPayload {
  type: string;
  data: Record<string, unknown>;
  metadata?: Record<string, string>;
}

export interface Event extends Timestamp {
  id: string;
  payload: EventPayload;
  status: EventStatus;
  retryCount: number;
  maxRetries: number;
  scheduledFor?: Date;
}

export interface EventFilter {
  status?: EventStatus;
  type?: string;
  fromDate?: Date;
  toDate?: Date;
}

export type EventHandler<T = unknown> = (event: Event) => Promise<T>;
"#,
        },
        FileTemplate {
            step: 3,
            path: "src/types/users.ts",
            purpose: "User type definitions",
            content: r#"// User type definitions
import { Timestamp } from './common';

export interface User extends Timestamp {
  id: string;
  email: string;
  name: string;
  role: UserRole;
  apiKey?: string;
}

export type UserRole = 'admin' | 'operator' | 'viewer';

export interface UserCredentials {
  email: string;
  password: string;
}

export interface UserSession {
  userId: string;
  token: string;
  expiresAt: Date;
}

export interface CreateUserRequest {
  email: string;
  name: string;
  password: string;
  role?: UserRole;
}
"#,
        },
        FileTemplate {
            step: 4,
            path: "src/config/database.ts",
            purpose: "Database configuration",
            content: r#"// Database configuration
export interface DatabaseConfig {
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  poolSize: number;
  ssl: boolean;
}

export function loadDatabaseConfig(): DatabaseConfig {
  const config: DatabaseConfig = {
    host: process.env.DB_HOST || 'localhost',
    port: parseInt(process.env.DB_PORT || '5432', 10),
    database: process.env.DB_NAME || 'events',
    username: process.env.DB_USER || 'postgres',
    password: process.env.DB_PASSWORD || '',
    poolSize: parseInt(process.env.DB_POOL_SIZE || '10', 10),
    ssl: process.env.DB_SSL === 'true',
  };

  validateConfig(config);
  return config;
}

function validateConfig(config: DatabaseConfig): void {
  if (!config.host) {
    throw new Error('Database host is required');
  }
  if (config.port < 1 || config.port > 65535) {
    throw new Error('Invalid database port');
  }
  if (!config.database) {
    throw new Error('Database name is required');
  }
}
"#,
        },
        FileTemplate {
            step: 5,
            path: "src/config/server.ts",
            purpose: "Server configuration",
            content: r#"// Server configuration
export interface ServerConfig {
  port: number;
  host: string;
  corsOrigins: string[];
  rateLimit: RateLimitConfig;
  timeout: number;
}

export interface RateLimitConfig {
  windowMs: number;
  maxRequests: number;
}

export function loadServerConfig(): ServerConfig {
  return {
    port: parseInt(process.env.PORT || '3000', 10),
    host: process.env.HOST || '0.0.0.0',
    corsOrigins: parseCorsOrigins(),
    rateLimit: {
      windowMs: 60000,
      maxRequests: parseInt(process.env.RATE_LIMIT || '100', 10),
    },
    timeout: parseInt(process.env.TIMEOUT || '30000', 10),
  };
}

function parseCorsOrigins(): string[] {
  const origins = process.env.CORS_ORIGINS || '*';
  if (origins === '*') {
    return ['*'];
  }
  return origins.split(',').map(o => o.trim());
}

export function validatePort(port: number): boolean {
  return port >= 1 && port <= 65535;
}
"#,
        },
        FileTemplate {
            step: 6,
            path: "src/utils/logger.ts",
            purpose: "Logging utilities",
            content: r#"// Logging utilities
export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

interface LogEntry {
  timestamp: string;
  level: LogLevel;
  message: string;
  context?: Record<string, unknown>;
}

class Logger {
  private minLevel: LogLevel;
  private levels: Record<LogLevel, number> = {
    debug: 0,
    info: 1,
    warn: 2,
    error: 3,
  };

  constructor(minLevel: LogLevel = 'info') {
    this.minLevel = minLevel;
  }

  private shouldLog(level: LogLevel): boolean {
    return this.levels[level] >= this.levels[this.minLevel];
  }

  private formatEntry(entry: LogEntry): string {
    const base = `[${entry.timestamp}] ${entry.level.toUpperCase()}: ${entry.message}`;
    if (entry.context) {
      return `${base} ${JSON.stringify(entry.context)}`;
    }
    return base;
  }

  log(level: LogLevel, message: string, context?: Record<string, unknown>): void {
    if (!this.shouldLog(level)) return;

    const entry: LogEntry = {
      timestamp: new Date().toISOString(),
      level,
      message,
      context,
    };
    console.log(this.formatEntry(entry));
  }

  debug(message: string, context?: Record<string, unknown>): void {
    this.log('debug', message, context);
  }

  info(message: string, context?: Record<string, unknown>): void {
    this.log('info', message, context);
  }

  warn(message: string, context?: Record<string, unknown>): void {
    this.log('warn', message, context);
  }

  error(message: string, context?: Record<string, unknown>): void {
    this.log('error', message, context);
  }
}

export const logger = new Logger(
  (process.env.LOG_LEVEL as LogLevel) || 'info'
);

export function createChildLogger(prefix: string): Logger {
  return new Logger();
}
"#,
        },
        FileTemplate {
            step: 7,
            path: "src/utils/validation.ts",
            purpose: "Validation utilities",
            content: r#"// Validation utilities
import { Result } from '../types/common';

export interface ValidationRule<T> {
  name: string;
  validate: (value: T) => boolean;
  message: string;
}

export function validate<T>(
  value: T,
  rules: ValidationRule<T>[]
): Result<T, string[]> {
  const errors: string[] = [];

  for (const rule of rules) {
    if (!rule.validate(value)) {
      errors.push(rule.message);
    }
  }

  if (errors.length > 0) {
    return { success: false, error: errors };
  }

  return { success: true, data: value };
}

export function isValidEmail(email: string): boolean {
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  return emailRegex.test(email);
}

export function isValidUUID(id: string): boolean {
  const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
  return uuidRegex.test(id);
}

export function isNonEmpty(value: string): boolean {
  return value.trim().length > 0;
}

export function isInRange(value: number, min: number, max: number): boolean {
  return value >= min && value <= max;
}

export function sanitizeString(input: string): string {
  return input.replace(/[<>\"'&]/g, '');
}
"#,
        },
        FileTemplate {
            step: 8,
            path: "src/utils/crypto.ts",
            purpose: "Cryptography utilities",
            content: r#"// Cryptography utilities
import { createHash, randomBytes } from 'crypto';

export function generateId(): string {
  return randomBytes(16).toString('hex');
}

export function generateApiKey(): string {
  const prefix = 'evt_';
  const key = randomBytes(24).toString('base64url');
  return `${prefix}${key}`;
}

export function hashPassword(password: string, salt: string): string {
  return createHash('sha256')
    .update(password + salt)
    .digest('hex');
}

export function generateSalt(): string {
  return randomBytes(16).toString('hex');
}

export function verifyPassword(
  password: string,
  salt: string,
  hash: string
): boolean {
  const computed = hashPassword(password, salt);
  return timingSafeEqual(computed, hash);
}

function timingSafeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) {
    return false;
  }

  let result = 0;
  for (let i = 0; i < a.length; i++) {
    result |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return result === 0;
}

export function generateToken(length: number = 32): string {
  return randomBytes(length).toString('hex');
}
"#,
        },
        FileTemplate {
            step: 9,
            path: "src/utils/retry.ts",
            purpose: "Retry logic utilities",
            content: r#"// Retry logic utilities
import { logger } from './logger';

export interface RetryConfig {
  maxAttempts: number;
  baseDelayMs: number;
  maxDelayMs: number;
  backoffMultiplier: number;
}

export const defaultRetryConfig: RetryConfig = {
  maxAttempts: 3,
  baseDelayMs: 1000,
  maxDelayMs: 30000,
  backoffMultiplier: 2,
};

export async function withRetry<T>(
  fn: () => Promise<T>,
  config: Partial<RetryConfig> = {}
): Promise<T> {
  const opts = { ...defaultRetryConfig, ...config };
  let lastError: Error | undefined;
  let delay = opts.baseDelayMs;

  for (let attempt = 1; attempt <= opts.maxAttempts; attempt++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error as Error;

      logger.warn(`Attempt ${attempt}/${opts.maxAttempts} failed`, {
        error: lastError.message,
        nextDelayMs: delay,
      });

      if (attempt < opts.maxAttempts) {
        await sleep(delay);
        delay = calculateNextDelay(delay, opts);
      }
    }
  }

  throw lastError;
}

function calculateNextDelay(current: number, config: RetryConfig): number {
  const next = current * config.backoffMultiplier;
  return Math.min(next, config.maxDelayMs);
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

export function isRetryableError(error: Error): boolean {
  const retryableCodes = ['ECONNRESET', 'ETIMEDOUT', 'ECONNREFUSED'];
  return retryableCodes.some(code => error.message.includes(code));
}
"#,
        },
        FileTemplate {
            step: 10,
            path: "src/utils/queue.ts",
            purpose: "In-memory queue implementation",
            content: r#"// In-memory queue implementation
import { Event, EventStatus } from '../types/events';
import { logger } from './logger';
import { generateId } from './crypto';

interface QueueItem {
  event: Event;
  addedAt: Date;
  attempts: number;
}

export class EventQueue {
  private items: Map<string, QueueItem> = new Map();
  private processing: Set<string> = new Set();
  private maxSize: number;

  constructor(maxSize: number = 10000) {
    this.maxSize = maxSize;
  }

  enqueue(event: Event): boolean {
    if (this.items.size >= this.maxSize) {
      logger.warn('Queue is full, rejecting event', { eventId: event.id });
      return false;
    }

    this.items.set(event.id, {
      event,
      addedAt: new Date(),
      attempts: 0,
    });

    logger.debug('Event enqueued', { eventId: event.id });
    return true;
  }

  dequeue(): Event | undefined {
    for (const [id, item] of this.items) {
      if (!this.processing.has(id) && this.isReady(item)) {
        this.processing.add(id);
        item.attempts++;
        return item.event;
      }
    }
    return undefined;
  }

  private isReady(item: QueueItem): boolean {
    const event = item.event;
    if (event.scheduledFor && event.scheduledFor > new Date()) {
      return false;
    }
    return event.status === 'pending';
  }

  complete(eventId: string): void {
    this.items.delete(eventId);
    this.processing.delete(eventId);
    logger.debug('Event completed', { eventId });
  }

  fail(eventId: string): void {
    const item = this.items.get(eventId);
    if (item) {
      item.event.status = 'failed';
      item.event.retryCount++;
    }
    this.processing.delete(eventId);
    logger.debug('Event failed', { eventId });
  }

  size(): number {
    return this.items.size;
  }

  processingCount(): number {
    return this.processing.size;
  }
}

export const globalQueue = new EventQueue();
"#,
        },

        // ============================================================
        // PHASE 2: Repository Layer (Steps 11-20)
        // ============================================================
        FileTemplate {
            step: 11,
            path: "src/db/connection.ts",
            purpose: "Database connection pool",
            content: r#"// Database connection pool
import { loadDatabaseConfig, DatabaseConfig } from '../config/database';
import { logger } from '../utils/logger';

interface Connection {
  id: string;
  inUse: boolean;
  createdAt: Date;
}

class ConnectionPool {
  private config: DatabaseConfig;
  private connections: Connection[] = [];
  private waitQueue: ((conn: Connection) => void)[] = [];

  constructor() {
    this.config = loadDatabaseConfig();
    this.initialize();
  }

  private initialize(): void {
    logger.info('Initializing database connection pool', {
      host: this.config.host,
      database: this.config.database,
      poolSize: this.config.poolSize,
    });

    for (let i = 0; i < this.config.poolSize; i++) {
      this.connections.push(this.createConnection());
    }
  }

  private createConnection(): Connection {
    return {
      id: `conn_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      inUse: false,
      createdAt: new Date(),
    };
  }

  async acquire(): Promise<Connection> {
    const available = this.connections.find(c => !c.inUse);

    if (available) {
      available.inUse = true;
      return available;
    }

    return new Promise((resolve) => {
      this.waitQueue.push(resolve);
    });
  }

  release(connection: Connection): void {
    connection.inUse = false;

    const waiting = this.waitQueue.shift();
    if (waiting) {
      connection.inUse = true;
      waiting(connection);
    }
  }

  async query<T>(sql: string, params: unknown[] = []): Promise<T[]> {
    const conn = await this.acquire();
    try {
      logger.debug('Executing query', { sql: sql.substring(0, 100) });
      // Simulate query execution
      return [] as T[];
    } finally {
      this.release(conn);
    }
  }

  getStats(): { total: number; inUse: number; waiting: number } {
    return {
      total: this.connections.length,
      inUse: this.connections.filter(c => c.inUse).length,
      waiting: this.waitQueue.length,
    };
  }
}

export const db = new ConnectionPool();
"#,
        },
        FileTemplate {
            step: 12,
            path: "src/repos/base.ts",
            purpose: "Base repository class",
            content: r#"// Base repository class
import { db } from '../db/connection';
import { Result, PaginatedResult } from '../types/common';
import { logger } from '../utils/logger';

export interface QueryOptions {
  limit?: number;
  offset?: number;
  orderBy?: string;
  order?: 'ASC' | 'DESC';
}

export abstract class BaseRepository<T, ID = string> {
  protected abstract tableName: string;
  protected abstract idField: string;

  async findById(id: ID): Promise<T | null> {
    const sql = `SELECT * FROM ${this.tableName} WHERE ${this.idField} = $1`;
    const results = await db.query<T>(sql, [id]);
    return results[0] || null;
  }

  async findAll(options: QueryOptions = {}): Promise<T[]> {
    const { limit = 100, offset = 0, orderBy, order = 'ASC' } = options;
    let sql = `SELECT * FROM ${this.tableName}`;

    if (orderBy) {
      sql += ` ORDER BY ${orderBy} ${order}`;
    }
    sql += ` LIMIT $1 OFFSET $2`;

    return db.query<T>(sql, [limit, offset]);
  }

  async findPaginated(
    options: QueryOptions = {}
  ): Promise<PaginatedResult<T>> {
    const items = await this.findAll(options);
    const total = await this.count();
    const page = Math.floor((options.offset || 0) / (options.limit || 100)) + 1;

    return {
      items,
      total,
      page,
      pageSize: options.limit || 100,
      hasMore: (options.offset || 0) + items.length < total,
    };
  }

  async count(): Promise<number> {
    const sql = `SELECT COUNT(*) as count FROM ${this.tableName}`;
    const results = await db.query<{ count: string }>(sql);
    return parseInt(results[0]?.count || '0', 10);
  }

  async exists(id: ID): Promise<boolean> {
    const entity = await this.findById(id);
    return entity !== null;
  }

  async delete(id: ID): Promise<boolean> {
    const sql = `DELETE FROM ${this.tableName} WHERE ${this.idField} = $1`;
    await db.query(sql, [id]);
    logger.info(`Deleted ${this.tableName}`, { id });
    return true;
  }

  protected logOperation(operation: string, data: Record<string, unknown>): void {
    logger.debug(`${this.tableName}.${operation}`, data);
  }
}
"#,
        },
        FileTemplate {
            step: 13,
            path: "src/repos/events.ts",
            purpose: "Event repository",
            content: r#"// Event repository
import { BaseRepository, QueryOptions } from './base';
import { Event, EventFilter, EventStatus } from '../types/events';
import { db } from '../db/connection';
import { generateId } from '../utils/crypto';
import { logger } from '../utils/logger';

export class EventRepository extends BaseRepository<Event> {
  protected tableName = 'events';
  protected idField = 'id';

  async create(event: Omit<Event, 'id' | 'createdAt' | 'updatedAt'>): Promise<Event> {
    const now = new Date();
    const newEvent: Event = {
      ...event,
      id: generateId(),
      createdAt: now,
      updatedAt: now,
    };

    const sql = `
      INSERT INTO events (id, payload, status, retry_count, max_retries, scheduled_for, created_at, updated_at)
      VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
    `;

    await db.query(sql, [
      newEvent.id,
      JSON.stringify(newEvent.payload),
      newEvent.status,
      newEvent.retryCount,
      newEvent.maxRetries,
      newEvent.scheduledFor,
      newEvent.createdAt,
      newEvent.updatedAt,
    ]);

    this.logOperation('create', { eventId: newEvent.id });
    return newEvent;
  }

  async updateStatus(id: string, status: EventStatus): Promise<Event | null> {
    const sql = `
      UPDATE events
      SET status = $1, updated_at = $2
      WHERE id = $3
    `;
    await db.query(sql, [status, new Date(), id]);
    this.logOperation('updateStatus', { id, status });
    return this.findById(id);
  }

  async findByFilter(filter: EventFilter, options: QueryOptions = {}): Promise<Event[]> {
    const conditions: string[] = [];
    const params: unknown[] = [];
    let paramIndex = 1;

    if (filter.status) {
      conditions.push(`status = $${paramIndex++}`);
      params.push(filter.status);
    }
    if (filter.type) {
      conditions.push(`payload->>'type' = $${paramIndex++}`);
      params.push(filter.type);
    }
    if (filter.fromDate) {
      conditions.push(`created_at >= $${paramIndex++}`);
      params.push(filter.fromDate);
    }
    if (filter.toDate) {
      conditions.push(`created_at <= $${paramIndex++}`);
      params.push(filter.toDate);
    }

    let sql = `SELECT * FROM events`;
    if (conditions.length > 0) {
      sql += ` WHERE ${conditions.join(' AND ')}`;
    }
    sql += ` LIMIT $${paramIndex++} OFFSET $${paramIndex}`;
    params.push(options.limit || 100, options.offset || 0);

    return db.query<Event>(sql, params);
  }

  async findPending(limit: number = 100): Promise<Event[]> {
    return this.findByFilter({ status: 'pending' }, { limit });
  }

  async incrementRetry(id: string): Promise<void> {
    const sql = `
      UPDATE events
      SET retry_count = retry_count + 1, updated_at = $1
      WHERE id = $2
    `;
    await db.query(sql, [new Date(), id]);
  }
}

export const eventRepository = new EventRepository();
"#,
        },
        FileTemplate {
            step: 14,
            path: "src/repos/users.ts",
            purpose: "User repository",
            content: r#"// User repository
import { BaseRepository } from './base';
import { User, UserRole, CreateUserRequest } from '../types/users';
import { db } from '../db/connection';
import { generateId, generateApiKey, hashPassword, generateSalt } from '../utils/crypto';
import { logger } from '../utils/logger';

export class UserRepository extends BaseRepository<User> {
  protected tableName = 'users';
  protected idField = 'id';

  async create(request: CreateUserRequest): Promise<User> {
    const now = new Date();
    const salt = generateSalt();
    const passwordHash = hashPassword(request.password, salt);

    const newUser: User = {
      id: generateId(),
      email: request.email,
      name: request.name,
      role: request.role || 'viewer',
      apiKey: generateApiKey(),
      createdAt: now,
      updatedAt: now,
    };

    const sql = `
      INSERT INTO users (id, email, name, role, api_key, password_hash, salt, created_at, updated_at)
      VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
    `;

    await db.query(sql, [
      newUser.id,
      newUser.email,
      newUser.name,
      newUser.role,
      newUser.apiKey,
      passwordHash,
      salt,
      newUser.createdAt,
      newUser.updatedAt,
    ]);

    this.logOperation('create', { userId: newUser.id, email: newUser.email });
    return newUser;
  }

  async findByEmail(email: string): Promise<User | null> {
    const sql = `SELECT * FROM users WHERE email = $1`;
    const results = await db.query<User>(sql, [email]);
    return results[0] || null;
  }

  async findByApiKey(apiKey: string): Promise<User | null> {
    const sql = `SELECT * FROM users WHERE api_key = $1`;
    const results = await db.query<User>(sql, [apiKey]);
    return results[0] || null;
  }

  async updateRole(id: string, role: UserRole): Promise<User | null> {
    const sql = `UPDATE users SET role = $1, updated_at = $2 WHERE id = $3`;
    await db.query(sql, [role, new Date(), id]);
    this.logOperation('updateRole', { id, role });
    return this.findById(id);
  }

  async regenerateApiKey(id: string): Promise<string> {
    const newKey = generateApiKey();
    const sql = `UPDATE users SET api_key = $1, updated_at = $2 WHERE id = $3`;
    await db.query(sql, [newKey, new Date(), id]);
    this.logOperation('regenerateApiKey', { id });
    return newKey;
  }

  async findByRole(role: UserRole): Promise<User[]> {
    const sql = `SELECT * FROM users WHERE role = $1`;
    return db.query<User>(sql, [role]);
  }
}

export const userRepository = new UserRepository();
"#,
        },
        FileTemplate {
            step: 15,
            path: "src/repos/sessions.ts",
            purpose: "Session repository",
            content: r#"// Session repository
import { BaseRepository } from './base';
import { UserSession } from '../types/users';
import { db } from '../db/connection';
import { generateToken } from '../utils/crypto';
import { logger } from '../utils/logger';

interface StoredSession extends UserSession {
  id: string;
  createdAt: Date;
}

export class SessionRepository extends BaseRepository<StoredSession> {
  protected tableName = 'sessions';
  protected idField = 'id';

  private sessionDurationMs = 24 * 60 * 60 * 1000; // 24 hours

  async create(userId: string): Promise<UserSession> {
    const now = new Date();
    const expiresAt = new Date(now.getTime() + this.sessionDurationMs);
    const token = generateToken(64);

    const session: StoredSession = {
      id: token,
      userId,
      token,
      expiresAt,
      createdAt: now,
    };

    const sql = `
      INSERT INTO sessions (id, user_id, token, expires_at, created_at)
      VALUES ($1, $2, $3, $4, $5)
    `;

    await db.query(sql, [
      session.id,
      session.userId,
      session.token,
      session.expiresAt,
      session.createdAt,
    ]);

    this.logOperation('create', { userId, expiresAt });
    return { userId, token, expiresAt };
  }

  async findByToken(token: string): Promise<UserSession | null> {
    const sql = `SELECT * FROM sessions WHERE token = $1 AND expires_at > $2`;
    const results = await db.query<StoredSession>(sql, [token, new Date()]);

    if (results.length === 0) {
      return null;
    }

    const session = results[0];
    return {
      userId: session.userId,
      token: session.token,
      expiresAt: session.expiresAt,
    };
  }

  async deleteByToken(token: string): Promise<boolean> {
    const sql = `DELETE FROM sessions WHERE token = $1`;
    await db.query(sql, [token]);
    this.logOperation('delete', { token: token.substring(0, 8) + '...' });
    return true;
  }

  async deleteExpired(): Promise<number> {
    const sql = `DELETE FROM sessions WHERE expires_at < $1`;
    await db.query(sql, [new Date()]);
    logger.info('Cleaned up expired sessions');
    return 0; // Would return count in real implementation
  }

  async deleteByUserId(userId: string): Promise<void> {
    const sql = `DELETE FROM sessions WHERE user_id = $1`;
    await db.query(sql, [userId]);
    this.logOperation('deleteByUserId', { userId });
  }

  async extend(token: string): Promise<UserSession | null> {
    const newExpiresAt = new Date(Date.now() + this.sessionDurationMs);
    const sql = `UPDATE sessions SET expires_at = $1 WHERE token = $2 AND expires_at > $3`;
    await db.query(sql, [newExpiresAt, token, new Date()]);
    return this.findByToken(token);
  }
}

export const sessionRepository = new SessionRepository();
"#,
        },
        FileTemplate {
            step: 16,
            path: "src/repos/webhooks.ts",
            purpose: "Webhook repository",
            content: r#"// Webhook repository
import { BaseRepository } from './base';
import { db } from '../db/connection';
import { generateId } from '../utils/crypto';
import { logger } from '../utils/logger';

export interface Webhook {
  id: string;
  url: string;
  secret: string;
  eventTypes: string[];
  isActive: boolean;
  userId: string;
  createdAt: Date;
  updatedAt: Date;
}

export interface CreateWebhookRequest {
  url: string;
  eventTypes: string[];
  userId: string;
}

export class WebhookRepository extends BaseRepository<Webhook> {
  protected tableName = 'webhooks';
  protected idField = 'id';

  async create(request: CreateWebhookRequest): Promise<Webhook> {
    const now = new Date();
    const webhook: Webhook = {
      id: generateId(),
      url: request.url,
      secret: generateId(),
      eventTypes: request.eventTypes,
      isActive: true,
      userId: request.userId,
      createdAt: now,
      updatedAt: now,
    };

    const sql = `
      INSERT INTO webhooks (id, url, secret, event_types, is_active, user_id, created_at, updated_at)
      VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
    `;

    await db.query(sql, [
      webhook.id,
      webhook.url,
      webhook.secret,
      JSON.stringify(webhook.eventTypes),
      webhook.isActive,
      webhook.userId,
      webhook.createdAt,
      webhook.updatedAt,
    ]);

    this.logOperation('create', { webhookId: webhook.id, url: webhook.url });
    return webhook;
  }

  async findByUserId(userId: string): Promise<Webhook[]> {
    const sql = `SELECT * FROM webhooks WHERE user_id = $1`;
    return db.query<Webhook>(sql, [userId]);
  }

  async findActiveByEventType(eventType: string): Promise<Webhook[]> {
    const sql = `
      SELECT * FROM webhooks
      WHERE is_active = true AND $1 = ANY(event_types)
    `;
    return db.query<Webhook>(sql, [eventType]);
  }

  async setActive(id: string, isActive: boolean): Promise<Webhook | null> {
    const sql = `UPDATE webhooks SET is_active = $1, updated_at = $2 WHERE id = $3`;
    await db.query(sql, [isActive, new Date(), id]);
    this.logOperation('setActive', { id, isActive });
    return this.findById(id);
  }

  async updateEventTypes(id: string, eventTypes: string[]): Promise<Webhook | null> {
    const sql = `UPDATE webhooks SET event_types = $1, updated_at = $2 WHERE id = $3`;
    await db.query(sql, [JSON.stringify(eventTypes), new Date(), id]);
    this.logOperation('updateEventTypes', { id, eventTypes });
    return this.findById(id);
  }
}

export const webhookRepository = new WebhookRepository();
"#,
        },
        FileTemplate {
            step: 17,
            path: "src/repos/metrics.ts",
            purpose: "Metrics repository",
            content: r#"// Metrics repository
import { db } from '../db/connection';
import { logger } from '../utils/logger';

export interface MetricPoint {
  timestamp: Date;
  name: string;
  value: number;
  tags: Record<string, string>;
}

export interface MetricAggregation {
  name: string;
  min: number;
  max: number;
  avg: number;
  sum: number;
  count: number;
}

export class MetricsRepository {
  async record(metric: MetricPoint): Promise<void> {
    const sql = `
      INSERT INTO metrics (timestamp, name, value, tags)
      VALUES ($1, $2, $3, $4)
    `;

    await db.query(sql, [
      metric.timestamp,
      metric.name,
      metric.value,
      JSON.stringify(metric.tags),
    ]);
  }

  async recordBatch(metrics: MetricPoint[]): Promise<void> {
    for (const metric of metrics) {
      await this.record(metric);
    }
    logger.debug('Recorded metrics batch', { count: metrics.length });
  }

  async getAggregation(
    name: string,
    from: Date,
    to: Date
  ): Promise<MetricAggregation> {
    const sql = `
      SELECT
        name,
        MIN(value) as min,
        MAX(value) as max,
        AVG(value) as avg,
        SUM(value) as sum,
        COUNT(*) as count
      FROM metrics
      WHERE name = $1 AND timestamp >= $2 AND timestamp <= $3
      GROUP BY name
    `;

    const results = await db.query<MetricAggregation>(sql, [name, from, to]);
    return results[0] || { name, min: 0, max: 0, avg: 0, sum: 0, count: 0 };
  }

  async getTimeSeries(
    name: string,
    from: Date,
    to: Date,
    intervalMinutes: number = 5
  ): Promise<{ timestamp: Date; value: number }[]> {
    const sql = `
      SELECT
        date_trunc('minute', timestamp) as timestamp,
        AVG(value) as value
      FROM metrics
      WHERE name = $1 AND timestamp >= $2 AND timestamp <= $3
      GROUP BY date_trunc('minute', timestamp)
      ORDER BY timestamp
    `;

    return db.query(sql, [name, from, to]);
  }

  async deleteOlderThan(days: number): Promise<number> {
    const cutoff = new Date(Date.now() - days * 24 * 60 * 60 * 1000);
    const sql = `DELETE FROM metrics WHERE timestamp < $1`;
    await db.query(sql, [cutoff]);
    logger.info('Deleted old metrics', { olderThan: cutoff });
    return 0;
  }
}

export const metricsRepository = new MetricsRepository();
"#,
        },
        FileTemplate {
            step: 18,
            path: "src/repos/audit.ts",
            purpose: "Audit log repository",
            content: r#"// Audit log repository
import { db } from '../db/connection';
import { generateId } from '../utils/crypto';
import { logger } from '../utils/logger';

export interface AuditEntry {
  id: string;
  action: string;
  entityType: string;
  entityId: string;
  userId: string;
  changes: Record<string, { old: unknown; new: unknown }>;
  ipAddress?: string;
  userAgent?: string;
  timestamp: Date;
}

export interface AuditFilter {
  userId?: string;
  entityType?: string;
  entityId?: string;
  action?: string;
  from?: Date;
  to?: Date;
}

export class AuditRepository {
  async log(entry: Omit<AuditEntry, 'id' | 'timestamp'>): Promise<AuditEntry> {
    const fullEntry: AuditEntry = {
      ...entry,
      id: generateId(),
      timestamp: new Date(),
    };

    const sql = `
      INSERT INTO audit_log (id, action, entity_type, entity_id, user_id, changes, ip_address, user_agent, timestamp)
      VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
    `;

    await db.query(sql, [
      fullEntry.id,
      fullEntry.action,
      fullEntry.entityType,
      fullEntry.entityId,
      fullEntry.userId,
      JSON.stringify(fullEntry.changes),
      fullEntry.ipAddress,
      fullEntry.userAgent,
      fullEntry.timestamp,
    ]);

    logger.debug('Audit entry logged', {
      action: entry.action,
      entityType: entry.entityType,
      entityId: entry.entityId,
    });

    return fullEntry;
  }

  async find(filter: AuditFilter, limit: number = 100): Promise<AuditEntry[]> {
    const conditions: string[] = [];
    const params: unknown[] = [];
    let paramIndex = 1;

    if (filter.userId) {
      conditions.push(`user_id = $${paramIndex++}`);
      params.push(filter.userId);
    }
    if (filter.entityType) {
      conditions.push(`entity_type = $${paramIndex++}`);
      params.push(filter.entityType);
    }
    if (filter.entityId) {
      conditions.push(`entity_id = $${paramIndex++}`);
      params.push(filter.entityId);
    }
    if (filter.action) {
      conditions.push(`action = $${paramIndex++}`);
      params.push(filter.action);
    }
    if (filter.from) {
      conditions.push(`timestamp >= $${paramIndex++}`);
      params.push(filter.from);
    }
    if (filter.to) {
      conditions.push(`timestamp <= $${paramIndex++}`);
      params.push(filter.to);
    }

    let sql = `SELECT * FROM audit_log`;
    if (conditions.length > 0) {
      sql += ` WHERE ${conditions.join(' AND ')}`;
    }
    sql += ` ORDER BY timestamp DESC LIMIT $${paramIndex}`;
    params.push(limit);

    return db.query<AuditEntry>(sql, params);
  }

  async getEntityHistory(entityType: string, entityId: string): Promise<AuditEntry[]> {
    return this.find({ entityType, entityId });
  }

  async getUserActivity(userId: string, limit: number = 50): Promise<AuditEntry[]> {
    return this.find({ userId }, limit);
  }
}

export const auditRepository = new AuditRepository();
"#,
        },
        FileTemplate {
            step: 19,
            path: "src/repos/subscriptions.ts",
            purpose: "Event subscription repository",
            content: r#"// Event subscription repository
import { BaseRepository } from './base';
import { db } from '../db/connection';
import { generateId } from '../utils/crypto';
import { logger } from '../utils/logger';

export interface Subscription {
  id: string;
  userId: string;
  eventPattern: string;
  callbackUrl: string;
  isActive: boolean;
  failureCount: number;
  lastDeliveryAt?: Date;
  createdAt: Date;
  updatedAt: Date;
}

export interface CreateSubscriptionRequest {
  userId: string;
  eventPattern: string;
  callbackUrl: string;
}

export class SubscriptionRepository extends BaseRepository<Subscription> {
  protected tableName = 'subscriptions';
  protected idField = 'id';

  async create(request: CreateSubscriptionRequest): Promise<Subscription> {
    const now = new Date();
    const subscription: Subscription = {
      id: generateId(),
      userId: request.userId,
      eventPattern: request.eventPattern,
      callbackUrl: request.callbackUrl,
      isActive: true,
      failureCount: 0,
      createdAt: now,
      updatedAt: now,
    };

    const sql = `
      INSERT INTO subscriptions (id, user_id, event_pattern, callback_url, is_active, failure_count, created_at, updated_at)
      VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
    `;

    await db.query(sql, [
      subscription.id,
      subscription.userId,
      subscription.eventPattern,
      subscription.callbackUrl,
      subscription.isActive,
      subscription.failureCount,
      subscription.createdAt,
      subscription.updatedAt,
    ]);

    this.logOperation('create', { subscriptionId: subscription.id });
    return subscription;
  }

  async findByUserId(userId: string): Promise<Subscription[]> {
    const sql = `SELECT * FROM subscriptions WHERE user_id = $1`;
    return db.query<Subscription>(sql, [userId]);
  }

  async findMatchingPattern(eventType: string): Promise<Subscription[]> {
    // Simple pattern matching - in real impl would use regex or LIKE
    const sql = `
      SELECT * FROM subscriptions
      WHERE is_active = true AND ($1 LIKE event_pattern OR event_pattern = '*')
    `;
    return db.query<Subscription>(sql, [eventType]);
  }

  async recordDelivery(id: string, success: boolean): Promise<void> {
    if (success) {
      const sql = `
        UPDATE subscriptions
        SET last_delivery_at = $1, failure_count = 0, updated_at = $1
        WHERE id = $2
      `;
      await db.query(sql, [new Date(), id]);
    } else {
      const sql = `
        UPDATE subscriptions
        SET failure_count = failure_count + 1, updated_at = $1
        WHERE id = $2
      `;
      await db.query(sql, [new Date(), id]);
    }
  }

  async deactivateUnhealthy(maxFailures: number = 5): Promise<string[]> {
    const sql = `
      UPDATE subscriptions
      SET is_active = false, updated_at = $1
      WHERE failure_count >= $2 AND is_active = true
      RETURNING id
    `;
    const results = await db.query<{ id: string }>(sql, [new Date(), maxFailures]);
    const ids = results.map(r => r.id);

    if (ids.length > 0) {
      logger.warn('Deactivated unhealthy subscriptions', { count: ids.length });
    }

    return ids;
  }
}

export const subscriptionRepository = new SubscriptionRepository();
"#,
        },
        FileTemplate {
            step: 20,
            path: "src/repos/index.ts",
            purpose: "Repository exports",
            content: r#"// Repository layer exports
export { eventRepository, EventRepository } from './events';
export { userRepository, UserRepository } from './users';
export { sessionRepository, SessionRepository } from './sessions';
export { webhookRepository, WebhookRepository, Webhook, CreateWebhookRequest } from './webhooks';
export { metricsRepository, MetricsRepository, MetricPoint, MetricAggregation } from './metrics';
export { auditRepository, AuditRepository, AuditEntry, AuditFilter } from './audit';
export { subscriptionRepository, SubscriptionRepository, Subscription, CreateSubscriptionRequest } from './subscriptions';
export { BaseRepository, QueryOptions } from './base';
export { db } from '../db/connection';
"#,
        },

        // ============================================================
        // PHASE 3: Service Layer (Steps 21-35)
        // ============================================================
        FileTemplate {
            step: 21,
            path: "src/services/auth.ts",
            purpose: "Authentication service",
            content: r#"// Authentication service
import { userRepository } from '../repos/users';
import { sessionRepository } from '../repos/sessions';
import { auditRepository } from '../repos/audit';
import { User, UserSession, UserCredentials } from '../types/users';
import { Result } from '../types/common';
import { verifyPassword, hashPassword, generateSalt } from '../utils/crypto';
import { logger } from '../utils/logger';

export interface AuthResult {
  user: User;
  session: UserSession;
}

export class AuthService {
  async login(credentials: UserCredentials): Promise<Result<AuthResult>> {
    const user = await userRepository.findByEmail(credentials.email);

    if (!user) {
      logger.warn('Login attempt for unknown user', { email: credentials.email });
      return { success: false, error: new Error('Invalid credentials') };
    }

    // In real impl, would fetch password hash from DB
    const isValid = await this.verifyCredentials(user.id, credentials.password);

    if (!isValid) {
      logger.warn('Invalid password attempt', { userId: user.id });
      await this.logLoginFailure(user.id);
      return { success: false, error: new Error('Invalid credentials') };
    }

    const session = await sessionRepository.create(user.id);
    await this.logLoginSuccess(user.id);

    return {
      success: true,
      data: { user, session },
    };
  }

  async logout(token: string): Promise<boolean> {
    const session = await sessionRepository.findByToken(token);

    if (session) {
      await sessionRepository.deleteByToken(token);
      await auditRepository.log({
        action: 'logout',
        entityType: 'session',
        entityId: token.substring(0, 8),
        userId: session.userId,
        changes: {},
      });
    }

    return true;
  }

  async validateSession(token: string): Promise<User | null> {
    const session = await sessionRepository.findByToken(token);

    if (!session) {
      return null;
    }

    if (new Date() > session.expiresAt) {
      await sessionRepository.deleteByToken(token);
      return null;
    }

    return userRepository.findById(session.userId);
  }

  async validateApiKey(apiKey: string): Promise<User | null> {
    return userRepository.findByApiKey(apiKey);
  }

  private async verifyCredentials(userId: string, password: string): Promise<boolean> {
    // Simulated - would fetch hash/salt from DB
    return password.length > 0;
  }

  private async logLoginSuccess(userId: string): Promise<void> {
    await auditRepository.log({
      action: 'login_success',
      entityType: 'user',
      entityId: userId,
      userId,
      changes: {},
    });
  }

  private async logLoginFailure(userId: string): Promise<void> {
    await auditRepository.log({
      action: 'login_failure',
      entityType: 'user',
      entityId: userId,
      userId,
      changes: {},
    });
  }
}

export const authService = new AuthService();
"#,
        },
        FileTemplate {
            step: 22,
            path: "src/services/events.ts",
            purpose: "Event service",
            content: r#"// Event service
import { eventRepository } from '../repos/events';
import { subscriptionRepository } from '../repos/subscriptions';
import { metricsRepository } from '../repos/metrics';
import { auditRepository } from '../repos/audit';
import { Event, EventPayload, EventFilter, EventStatus } from '../types/events';
import { Result, PaginatedResult } from '../types/common';
import { logger } from '../utils/logger';
import { globalQueue } from '../utils/queue';

export class EventService {
  async createEvent(
    payload: EventPayload,
    userId: string,
    options: { maxRetries?: number; scheduledFor?: Date } = {}
  ): Promise<Result<Event>> {
    try {
      const event = await eventRepository.create({
        payload,
        status: 'pending',
        retryCount: 0,
        maxRetries: options.maxRetries || 3,
        scheduledFor: options.scheduledFor,
      });

      globalQueue.enqueue(event);

      await this.recordMetric('event.created', 1, { type: payload.type });
      await this.logEventAction('create', event.id, userId);

      logger.info('Event created', { eventId: event.id, type: payload.type });
      return { success: true, data: event };
    } catch (error) {
      logger.error('Failed to create event', { error: (error as Error).message });
      return { success: false, error: error as Error };
    }
  }

  async getEvent(id: string): Promise<Event | null> {
    return eventRepository.findById(id);
  }

  async listEvents(
    filter: EventFilter,
    page: number = 1,
    pageSize: number = 20
  ): Promise<PaginatedResult<Event>> {
    const offset = (page - 1) * pageSize;
    const events = await eventRepository.findByFilter(filter, {
      limit: pageSize,
      offset
    });

    const total = await eventRepository.count();

    return {
      items: events,
      total,
      page,
      pageSize,
      hasMore: offset + events.length < total,
    };
  }

  async updateStatus(
    id: string,
    status: EventStatus,
    userId: string
  ): Promise<Result<Event>> {
    const event = await eventRepository.findById(id);

    if (!event) {
      return { success: false, error: new Error('Event not found') };
    }

    const oldStatus = event.status;
    const updated = await eventRepository.updateStatus(id, status);

    if (!updated) {
      return { success: false, error: new Error('Update failed') };
    }

    await this.logEventAction('update_status', id, userId, { oldStatus, newStatus: status });
    await this.recordMetric('event.status_changed', 1, { from: oldStatus, to: status });

    return { success: true, data: updated };
  }

  async processNextEvent(): Promise<Event | null> {
    const event = globalQueue.dequeue();

    if (!event) {
      return null;
    }

    await eventRepository.updateStatus(event.id, 'processing');
    return event;
  }

  async completeEvent(id: string): Promise<void> {
    await eventRepository.updateStatus(id, 'completed');
    globalQueue.complete(id);
    await this.recordMetric('event.completed', 1);
  }

  async failEvent(id: string): Promise<void> {
    const event = await eventRepository.findById(id);

    if (event && event.retryCount < event.maxRetries) {
      await eventRepository.incrementRetry(id);
      await eventRepository.updateStatus(id, 'pending');
      globalQueue.fail(id);
    } else {
      await eventRepository.updateStatus(id, 'failed');
      globalQueue.fail(id);
      await this.recordMetric('event.failed', 1);
    }
  }

  private async recordMetric(name: string, value: number, tags: Record<string, string> = {}): Promise<void> {
    await metricsRepository.record({
      timestamp: new Date(),
      name,
      value,
      tags,
    });
  }

  private async logEventAction(
    action: string,
    eventId: string,
    userId: string,
    changes: Record<string, unknown> = {}
  ): Promise<void> {
    await auditRepository.log({
      action,
      entityType: 'event',
      entityId: eventId,
      userId,
      changes: Object.fromEntries(
        Object.entries(changes).map(([k, v]) => [k, { old: null, new: v }])
      ),
    });
  }
}

export const eventService = new EventService();
"#,
        },
        FileTemplate {
            step: 23,
            path: "src/services/users.ts",
            purpose: "User service",
            content: r#"// User service
import { userRepository } from '../repos/users';
import { sessionRepository } from '../repos/sessions';
import { auditRepository } from '../repos/audit';
import { User, CreateUserRequest, UserRole } from '../types/users';
import { Result, PaginatedResult } from '../types/common';
import { isValidEmail, isNonEmpty } from '../utils/validation';
import { logger } from '../utils/logger';

export class UserService {
  async createUser(
    request: CreateUserRequest,
    createdBy: string
  ): Promise<Result<User>> {
    // Validate input
    const validationErrors = this.validateCreateRequest(request);
    if (validationErrors.length > 0) {
      return { success: false, error: new Error(validationErrors.join(', ')) };
    }

    // Check for existing user
    const existing = await userRepository.findByEmail(request.email);
    if (existing) {
      return { success: false, error: new Error('Email already in use') };
    }

    try {
      const user = await userRepository.create(request);

      await auditRepository.log({
        action: 'user.create',
        entityType: 'user',
        entityId: user.id,
        userId: createdBy,
        changes: {
          email: { old: null, new: user.email },
          role: { old: null, new: user.role },
        },
      });

      logger.info('User created', { userId: user.id, email: user.email });
      return { success: true, data: user };
    } catch (error) {
      logger.error('Failed to create user', { error: (error as Error).message });
      return { success: false, error: error as Error };
    }
  }

  async getUser(id: string): Promise<User | null> {
    return userRepository.findById(id);
  }

  async getUserByEmail(email: string): Promise<User | null> {
    return userRepository.findByEmail(email);
  }

  async listUsers(page: number = 1, pageSize: number = 20): Promise<PaginatedResult<User>> {
    const result = await userRepository.findPaginated({
      limit: pageSize,
      offset: (page - 1) * pageSize,
      orderBy: 'created_at',
      order: 'DESC',
    });
    return result;
  }

  async updateRole(
    userId: string,
    newRole: UserRole,
    updatedBy: string
  ): Promise<Result<User>> {
    const user = await userRepository.findById(userId);

    if (!user) {
      return { success: false, error: new Error('User not found') };
    }

    const oldRole = user.role;
    const updated = await userRepository.updateRole(userId, newRole);

    if (!updated) {
      return { success: false, error: new Error('Update failed') };
    }

    await auditRepository.log({
      action: 'user.update_role',
      entityType: 'user',
      entityId: userId,
      userId: updatedBy,
      changes: { role: { old: oldRole, new: newRole } },
    });

    logger.info('User role updated', { userId, oldRole, newRole });
    return { success: true, data: updated };
  }

  async regenerateApiKey(userId: string, requestedBy: string): Promise<Result<string>> {
    const user = await userRepository.findById(userId);

    if (!user) {
      return { success: false, error: new Error('User not found') };
    }

    const newKey = await userRepository.regenerateApiKey(userId);

    await auditRepository.log({
      action: 'user.regenerate_api_key',
      entityType: 'user',
      entityId: userId,
      userId: requestedBy,
      changes: {},
    });

    logger.info('API key regenerated', { userId });
    return { success: true, data: newKey };
  }

  async deleteUser(userId: string, deletedBy: string): Promise<Result<boolean>> {
    const user = await userRepository.findById(userId);

    if (!user) {
      return { success: false, error: new Error('User not found') };
    }

    // Clean up sessions first
    await sessionRepository.deleteByUserId(userId);
    await userRepository.delete(userId);

    await auditRepository.log({
      action: 'user.delete',
      entityType: 'user',
      entityId: userId,
      userId: deletedBy,
      changes: { deleted: { old: false, new: true } },
    });

    logger.info('User deleted', { userId, deletedBy });
    return { success: true, data: true };
  }

  private validateCreateRequest(request: CreateUserRequest): string[] {
    const errors: string[] = [];

    if (!isValidEmail(request.email)) {
      errors.push('Invalid email format');
    }
    if (!isNonEmpty(request.name)) {
      errors.push('Name is required');
    }
    if (request.password.length < 8) {
      errors.push('Password must be at least 8 characters');
    }

    return errors;
  }
}

export const userService = new UserService();
"#,
        },
        FileTemplate {
            step: 24,
            path: "src/services/webhooks.ts",
            purpose: "Webhook service",
            content: r#"// Webhook service
import { webhookRepository, Webhook, CreateWebhookRequest } from '../repos/webhooks';
import { auditRepository } from '../repos/audit';
import { metricsRepository } from '../repos/metrics';
import { Event } from '../types/events';
import { Result } from '../types/common';
import { withRetry } from '../utils/retry';
import { logger } from '../utils/logger';

interface WebhookDeliveryResult {
  webhookId: string;
  success: boolean;
  statusCode?: number;
  error?: string;
  durationMs: number;
}

export class WebhookService {
  async createWebhook(
    request: CreateWebhookRequest,
    userId: string
  ): Promise<Result<Webhook>> {
    try {
      // Validate URL
      if (!this.isValidUrl(request.url)) {
        return { success: false, error: new Error('Invalid webhook URL') };
      }

      const webhook = await webhookRepository.create(request);

      await auditRepository.log({
        action: 'webhook.create',
        entityType: 'webhook',
        entityId: webhook.id,
        userId,
        changes: { url: { old: null, new: webhook.url } },
      });

      logger.info('Webhook created', { webhookId: webhook.id, url: webhook.url });
      return { success: true, data: webhook };
    } catch (error) {
      return { success: false, error: error as Error };
    }
  }

  async deliverEvent(event: Event): Promise<WebhookDeliveryResult[]> {
    const webhooks = await webhookRepository.findActiveByEventType(event.payload.type);
    const results: WebhookDeliveryResult[] = [];

    for (const webhook of webhooks) {
      const result = await this.deliverToWebhook(webhook, event);
      results.push(result);
    }

    await this.recordDeliveryMetrics(results);
    return results;
  }

  private async deliverToWebhook(
    webhook: Webhook,
    event: Event
  ): Promise<WebhookDeliveryResult> {
    const startTime = Date.now();

    try {
      const result = await withRetry(
        () => this.sendWebhookRequest(webhook, event),
        { maxAttempts: 3, baseDelayMs: 500 }
      );

      const durationMs = Date.now() - startTime;

      logger.debug('Webhook delivered', {
        webhookId: webhook.id,
        eventId: event.id,
        durationMs,
      });

      return {
        webhookId: webhook.id,
        success: true,
        statusCode: 200,
        durationMs,
      };
    } catch (error) {
      const durationMs = Date.now() - startTime;
      const errorMessage = (error as Error).message;

      logger.warn('Webhook delivery failed', {
        webhookId: webhook.id,
        eventId: event.id,
        error: errorMessage,
      });

      return {
        webhookId: webhook.id,
        success: false,
        error: errorMessage,
        durationMs,
      };
    }
  }

  private async sendWebhookRequest(webhook: Webhook, event: Event): Promise<void> {
    const payload = {
      event: {
        id: event.id,
        type: event.payload.type,
        data: event.payload.data,
        timestamp: event.createdAt,
      },
      deliveredAt: new Date().toISOString(),
    };

    // Simulated HTTP request
    logger.debug('Sending webhook', { url: webhook.url, eventType: event.payload.type });
  }

  async toggleWebhook(
    id: string,
    isActive: boolean,
    userId: string
  ): Promise<Result<Webhook>> {
    const webhook = await webhookRepository.setActive(id, isActive);

    if (!webhook) {
      return { success: false, error: new Error('Webhook not found') };
    }

    await auditRepository.log({
      action: isActive ? 'webhook.enable' : 'webhook.disable',
      entityType: 'webhook',
      entityId: id,
      userId,
      changes: { isActive: { old: !isActive, new: isActive } },
    });

    return { success: true, data: webhook };
  }

  async getUserWebhooks(userId: string): Promise<Webhook[]> {
    return webhookRepository.findByUserId(userId);
  }

  private isValidUrl(url: string): boolean {
    try {
      const parsed = new URL(url);
      return parsed.protocol === 'https:' || parsed.protocol === 'http:';
    } catch {
      return false;
    }
  }

  private async recordDeliveryMetrics(results: WebhookDeliveryResult[]): Promise<void> {
    const successful = results.filter(r => r.success).length;
    const failed = results.length - successful;
    const avgDuration = results.reduce((sum, r) => sum + r.durationMs, 0) / results.length;

    await metricsRepository.recordBatch([
      { timestamp: new Date(), name: 'webhook.delivered', value: successful, tags: { status: 'success' } },
      { timestamp: new Date(), name: 'webhook.delivered', value: failed, tags: { status: 'failed' } },
      { timestamp: new Date(), name: 'webhook.duration_ms', value: avgDuration, tags: {} },
    ]);
  }
}

export const webhookService = new WebhookService();
"#,
        },
        FileTemplate {
            step: 25,
            path: "src/services/subscriptions.ts",
            purpose: "Subscription service",
            content: r#"// Subscription service
import { subscriptionRepository, Subscription, CreateSubscriptionRequest } from '../repos/subscriptions';
import { auditRepository } from '../repos/audit';
import { Event } from '../types/events';
import { Result } from '../types/common';
import { withRetry } from '../utils/retry';
import { logger } from '../utils/logger';

interface DeliveryResult {
  subscriptionId: string;
  success: boolean;
  error?: string;
}

export class SubscriptionService {
  async createSubscription(
    request: CreateSubscriptionRequest,
    userId: string
  ): Promise<Result<Subscription>> {
    try {
      // Validate event pattern
      if (!this.isValidPattern(request.eventPattern)) {
        return { success: false, error: new Error('Invalid event pattern') };
      }

      const subscription = await subscriptionRepository.create(request);

      await auditRepository.log({
        action: 'subscription.create',
        entityType: 'subscription',
        entityId: subscription.id,
        userId,
        changes: {
          pattern: { old: null, new: subscription.eventPattern },
          callbackUrl: { old: null, new: subscription.callbackUrl },
        },
      });

      logger.info('Subscription created', {
        subscriptionId: subscription.id,
        pattern: subscription.eventPattern,
      });

      return { success: true, data: subscription };
    } catch (error) {
      return { success: false, error: error as Error };
    }
  }

  async deliverToSubscribers(event: Event): Promise<DeliveryResult[]> {
    const subscriptions = await subscriptionRepository.findMatchingPattern(
      event.payload.type
    );

    const results: DeliveryResult[] = [];

    for (const subscription of subscriptions) {
      const result = await this.deliverToSubscription(subscription, event);
      results.push(result);

      // Record delivery result
      await subscriptionRepository.recordDelivery(subscription.id, result.success);
    }

    // Cleanup unhealthy subscriptions
    await subscriptionRepository.deactivateUnhealthy();

    return results;
  }

  private async deliverToSubscription(
    subscription: Subscription,
    event: Event
  ): Promise<DeliveryResult> {
    try {
      await withRetry(
        () => this.sendCallback(subscription, event),
        { maxAttempts: 2, baseDelayMs: 200 }
      );

      return { subscriptionId: subscription.id, success: true };
    } catch (error) {
      return {
        subscriptionId: subscription.id,
        success: false,
        error: (error as Error).message,
      };
    }
  }

  private async sendCallback(subscription: Subscription, event: Event): Promise<void> {
    const payload = {
      eventType: event.payload.type,
      eventId: event.id,
      data: event.payload.data,
      timestamp: event.createdAt,
    };

    // Simulated callback
    logger.debug('Sending subscription callback', {
      subscriptionId: subscription.id,
      url: subscription.callbackUrl,
    });
  }

  async getUserSubscriptions(userId: string): Promise<Subscription[]> {
    return subscriptionRepository.findByUserId(userId);
  }

  async deleteSubscription(id: string, userId: string): Promise<Result<boolean>> {
    const subscription = await subscriptionRepository.findById(id);

    if (!subscription) {
      return { success: false, error: new Error('Subscription not found') };
    }

    if (subscription.userId !== userId) {
      return { success: false, error: new Error('Not authorized') };
    }

    await subscriptionRepository.delete(id);

    await auditRepository.log({
      action: 'subscription.delete',
      entityType: 'subscription',
      entityId: id,
      userId,
      changes: {},
    });

    return { success: true, data: true };
  }

  private isValidPattern(pattern: string): boolean {
    // Allow wildcards and event type patterns
    return /^[\w.*-]+$/.test(pattern);
  }
}

export const subscriptionService = new SubscriptionService();
"#,
        },
        FileTemplate {
            step: 26,
            path: "src/services/metrics.ts",
            purpose: "Metrics service",
            content: r#"// Metrics service
import { metricsRepository, MetricAggregation } from '../repos/metrics';
import { eventRepository } from '../repos/events';
import { userRepository } from '../repos/users';
import { logger } from '../utils/logger';

export interface DashboardMetrics {
  events: EventMetrics;
  users: UserMetrics;
  system: SystemMetrics;
}

export interface EventMetrics {
  totalEvents: number;
  pendingEvents: number;
  completedToday: number;
  failedToday: number;
  avgProcessingTimeMs: number;
}

export interface UserMetrics {
  totalUsers: number;
  activeToday: number;
  newThisWeek: number;
}

export interface SystemMetrics {
  queueSize: number;
  avgLatencyMs: number;
  errorRate: number;
}

export class MetricsService {
  async getDashboardMetrics(): Promise<DashboardMetrics> {
    const [events, users, system] = await Promise.all([
      this.getEventMetrics(),
      this.getUserMetrics(),
      this.getSystemMetrics(),
    ]);

    return { events, users, system };
  }

  async getEventMetrics(): Promise<EventMetrics> {
    const today = new Date();
    today.setHours(0, 0, 0, 0);

    const totalEvents = await eventRepository.count();
    const pendingEvents = (await eventRepository.findByFilter({ status: 'pending' })).length;

    const processingTime = await metricsRepository.getAggregation(
      'event.processing_time_ms',
      today,
      new Date()
    );

    return {
      totalEvents,
      pendingEvents,
      completedToday: 0, // Would calculate from actual data
      failedToday: 0,
      avgProcessingTimeMs: processingTime.avg,
    };
  }

  async getUserMetrics(): Promise<UserMetrics> {
    const totalUsers = await userRepository.count();

    return {
      totalUsers,
      activeToday: 0, // Would calculate from session data
      newThisWeek: 0,
    };
  }

  async getSystemMetrics(): Promise<SystemMetrics> {
    const now = new Date();
    const hourAgo = new Date(now.getTime() - 60 * 60 * 1000);

    const latency = await metricsRepository.getAggregation(
      'request.latency_ms',
      hourAgo,
      now
    );

    const errors = await metricsRepository.getAggregation(
      'request.error',
      hourAgo,
      now
    );

    const total = await metricsRepository.getAggregation(
      'request.total',
      hourAgo,
      now
    );

    return {
      queueSize: 0,
      avgLatencyMs: latency.avg,
      errorRate: total.count > 0 ? errors.sum / total.sum : 0,
    };
  }

  async getMetricTimeSeries(
    metricName: string,
    hours: number = 24
  ): Promise<{ timestamp: Date; value: number }[]> {
    const now = new Date();
    const from = new Date(now.getTime() - hours * 60 * 60 * 1000);

    return metricsRepository.getTimeSeries(metricName, from, now);
  }

  async recordRequestMetric(
    path: string,
    method: string,
    statusCode: number,
    durationMs: number
  ): Promise<void> {
    const isError = statusCode >= 400;
    const tags = { path, method, status: String(statusCode) };

    await metricsRepository.recordBatch([
      { timestamp: new Date(), name: 'request.total', value: 1, tags },
      { timestamp: new Date(), name: 'request.latency_ms', value: durationMs, tags },
      ...(isError ? [{ timestamp: new Date(), name: 'request.error', value: 1, tags }] : []),
    ]);
  }

  async cleanupOldMetrics(retentionDays: number = 30): Promise<void> {
    const deleted = await metricsRepository.deleteOlderThan(retentionDays);
    logger.info('Cleaned up old metrics', { deleted, retentionDays });
  }
}

export const metricsService = new MetricsService();
"#,
        },
        FileTemplate {
            step: 27,
            path: "src/services/processor.ts",
            purpose: "Event processor service",
            content: r#"// Event processor service
import { eventService } from './events';
import { webhookService } from './webhooks';
import { subscriptionService } from './subscriptions';
import { metricsService } from './metrics';
import { Event, EventHandler } from '../types/events';
import { logger } from '../utils/logger';
import { withRetry } from '../utils/retry';

interface ProcessorConfig {
  concurrency: number;
  pollIntervalMs: number;
  shutdownTimeoutMs: number;
}

const defaultConfig: ProcessorConfig = {
  concurrency: 5,
  pollIntervalMs: 100,
  shutdownTimeoutMs: 30000,
};

export class EventProcessor {
  private config: ProcessorConfig;
  private handlers: Map<string, EventHandler> = new Map();
  private isRunning = false;
  private activeWorkers = 0;

  constructor(config: Partial<ProcessorConfig> = {}) {
    this.config = { ...defaultConfig, ...config };
  }

  registerHandler(eventType: string, handler: EventHandler): void {
    this.handlers.set(eventType, handler);
    logger.info('Handler registered', { eventType });
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      logger.warn('Processor already running');
      return;
    }

    this.isRunning = true;
    logger.info('Event processor started', { concurrency: this.config.concurrency });

    // Start worker loops
    const workers: Promise<void>[] = [];
    for (let i = 0; i < this.config.concurrency; i++) {
      workers.push(this.workerLoop(i));
    }

    await Promise.all(workers);
  }

  async stop(): Promise<void> {
    logger.info('Stopping event processor');
    this.isRunning = false;

    // Wait for active workers to finish
    const deadline = Date.now() + this.config.shutdownTimeoutMs;
    while (this.activeWorkers > 0 && Date.now() < deadline) {
      await this.sleep(100);
    }

    if (this.activeWorkers > 0) {
      logger.warn('Shutdown timeout, workers still active', { count: this.activeWorkers });
    }

    logger.info('Event processor stopped');
  }

  private async workerLoop(workerId: number): Promise<void> {
    while (this.isRunning) {
      try {
        const event = await eventService.processNextEvent();

        if (!event) {
          await this.sleep(this.config.pollIntervalMs);
          continue;
        }

        this.activeWorkers++;
        await this.processEvent(event, workerId);
        this.activeWorkers--;
      } catch (error) {
        logger.error('Worker error', { workerId, error: (error as Error).message });
        this.activeWorkers--;
      }
    }
  }

  private async processEvent(event: Event, workerId: number): Promise<void> {
    const startTime = Date.now();
    const eventType = event.payload.type;

    logger.debug('Processing event', { workerId, eventId: event.id, type: eventType });

    try {
      // Execute type-specific handler
      const handler = this.handlers.get(eventType);
      if (handler) {
        await withRetry(() => handler(event), { maxAttempts: 2 });
      }

      // Deliver to webhooks and subscriptions
      await Promise.all([
        webhookService.deliverEvent(event),
        subscriptionService.deliverToSubscribers(event),
      ]);

      await eventService.completeEvent(event.id);

      const durationMs = Date.now() - startTime;
      await metricsService.recordRequestMetric('/internal/process', 'POST', 200, durationMs);

      logger.debug('Event processed', { eventId: event.id, durationMs });
    } catch (error) {
      logger.error('Event processing failed', {
        eventId: event.id,
        error: (error as Error).message,
      });
      await eventService.failEvent(event.id);
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

export const eventProcessor = new EventProcessor();
"#,
        },
        FileTemplate {
            step: 28,
            path: "src/services/scheduler.ts",
            purpose: "Event scheduler service",
            content: r#"// Event scheduler service
import { eventService } from './events';
import { eventRepository } from '../repos/events';
import { metricsRepository } from '../repos/metrics';
import { logger } from '../utils/logger';

interface ScheduledJob {
  id: string;
  name: string;
  cronExpression: string;
  handler: () => Promise<void>;
  lastRun?: Date;
  nextRun: Date;
  isActive: boolean;
}

export class EventScheduler {
  private jobs: Map<string, ScheduledJob> = new Map();
  private isRunning = false;
  private checkIntervalMs = 1000;

  registerJob(
    name: string,
    cronExpression: string,
    handler: () => Promise<void>
  ): string {
    const id = `job_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    const nextRun = this.calculateNextRun(cronExpression);

    this.jobs.set(id, {
      id,
      name,
      cronExpression,
      handler,
      nextRun,
      isActive: true,
    });

    logger.info('Scheduled job registered', { id, name, nextRun });
    return id;
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      return;
    }

    this.isRunning = true;
    logger.info('Scheduler started');

    while (this.isRunning) {
      await this.checkAndRunDueJobs();
      await this.sleep(this.checkIntervalMs);
    }
  }

  stop(): void {
    this.isRunning = false;
    logger.info('Scheduler stopped');
  }

  private async checkAndRunDueJobs(): Promise<void> {
    const now = new Date();

    for (const job of this.jobs.values()) {
      if (!job.isActive || job.nextRun > now) {
        continue;
      }

      try {
        await this.runJob(job);
      } catch (error) {
        logger.error('Job execution failed', {
          jobId: job.id,
          name: job.name,
          error: (error as Error).message,
        });
      }
    }
  }

  private async runJob(job: ScheduledJob): Promise<void> {
    const startTime = Date.now();
    logger.debug('Running scheduled job', { id: job.id, name: job.name });

    await job.handler();

    job.lastRun = new Date();
    job.nextRun = this.calculateNextRun(job.cronExpression);

    const durationMs = Date.now() - startTime;
    await metricsRepository.record({
      timestamp: new Date(),
      name: 'scheduler.job_duration_ms',
      value: durationMs,
      tags: { job: job.name },
    });

    logger.debug('Job completed', { id: job.id, durationMs, nextRun: job.nextRun });
  }

  private calculateNextRun(cronExpression: string): Date {
    // Simplified: just add 1 minute for demo
    return new Date(Date.now() + 60000);
  }

  pauseJob(id: string): boolean {
    const job = this.jobs.get(id);
    if (job) {
      job.isActive = false;
      logger.info('Job paused', { id, name: job.name });
      return true;
    }
    return false;
  }

  resumeJob(id: string): boolean {
    const job = this.jobs.get(id);
    if (job) {
      job.isActive = true;
      job.nextRun = this.calculateNextRun(job.cronExpression);
      logger.info('Job resumed', { id, name: job.name });
      return true;
    }
    return false;
  }

  getJobStatus(): Array<{
    id: string;
    name: string;
    isActive: boolean;
    lastRun?: Date;
    nextRun: Date;
  }> {
    return Array.from(this.jobs.values()).map(job => ({
      id: job.id,
      name: job.name,
      isActive: job.isActive,
      lastRun: job.lastRun,
      nextRun: job.nextRun,
    }));
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

export const scheduler = new EventScheduler();

// Register default cleanup jobs
scheduler.registerJob('cleanup-expired-sessions', '0 * * * *', async () => {
  const { sessionRepository } = await import('../repos/sessions');
  await sessionRepository.deleteExpired();
});

scheduler.registerJob('cleanup-old-metrics', '0 0 * * *', async () => {
  await metricsRepository.deleteOlderThan(30);
});
"#,
        },
        FileTemplate {
            step: 29,
            path: "src/services/notifications.ts",
            purpose: "Notification service",
            content: r#"// Notification service
import { logger } from '../utils/logger';
import { metricsRepository } from '../repos/metrics';

export type NotificationChannel = 'email' | 'slack' | 'webhook';

export interface NotificationPayload {
  subject: string;
  message: string;
  metadata?: Record<string, unknown>;
}

export interface NotificationRecipient {
  channel: NotificationChannel;
  address: string;
}

export interface NotificationResult {
  recipient: NotificationRecipient;
  success: boolean;
  error?: string;
  sentAt: Date;
}

export class NotificationService {
  private emailEnabled: boolean;
  private slackEnabled: boolean;

  constructor() {
    this.emailEnabled = process.env.EMAIL_ENABLED === 'true';
    this.slackEnabled = !!process.env.SLACK_WEBHOOK_URL;
  }

  async send(
    payload: NotificationPayload,
    recipients: NotificationRecipient[]
  ): Promise<NotificationResult[]> {
    const results: NotificationResult[] = [];

    for (const recipient of recipients) {
      const result = await this.sendToRecipient(payload, recipient);
      results.push(result);
    }

    await this.recordMetrics(results);
    return results;
  }

  private async sendToRecipient(
    payload: NotificationPayload,
    recipient: NotificationRecipient
  ): Promise<NotificationResult> {
    try {
      switch (recipient.channel) {
        case 'email':
          await this.sendEmail(recipient.address, payload);
          break;
        case 'slack':
          await this.sendSlack(recipient.address, payload);
          break;
        case 'webhook':
          await this.sendWebhook(recipient.address, payload);
          break;
      }

      return {
        recipient,
        success: true,
        sentAt: new Date(),
      };
    } catch (error) {
      logger.error('Notification failed', {
        channel: recipient.channel,
        error: (error as Error).message,
      });

      return {
        recipient,
        success: false,
        error: (error as Error).message,
        sentAt: new Date(),
      };
    }
  }

  private async sendEmail(address: string, payload: NotificationPayload): Promise<void> {
    if (!this.emailEnabled) {
      throw new Error('Email notifications not configured');
    }

    logger.debug('Sending email notification', {
      to: address,
      subject: payload.subject,
    });

    // Simulated email send
  }

  private async sendSlack(channel: string, payload: NotificationPayload): Promise<void> {
    if (!this.slackEnabled) {
      throw new Error('Slack notifications not configured');
    }

    logger.debug('Sending Slack notification', {
      channel,
      subject: payload.subject,
    });

    // Simulated Slack webhook
  }

  private async sendWebhook(url: string, payload: NotificationPayload): Promise<void> {
    logger.debug('Sending webhook notification', {
      url,
      subject: payload.subject,
    });

    // Simulated webhook
  }

  private async recordMetrics(results: NotificationResult[]): Promise<void> {
    const byChannel: Record<string, { success: number; failed: number }> = {};

    for (const result of results) {
      const channel = result.recipient.channel;
      if (!byChannel[channel]) {
        byChannel[channel] = { success: 0, failed: 0 };
      }
      if (result.success) {
        byChannel[channel].success++;
      } else {
        byChannel[channel].failed++;
      }
    }

    for (const [channel, counts] of Object.entries(byChannel)) {
      await metricsRepository.recordBatch([
        {
          timestamp: new Date(),
          name: 'notification.sent',
          value: counts.success,
          tags: { channel, status: 'success' },
        },
        {
          timestamp: new Date(),
          name: 'notification.sent',
          value: counts.failed,
          tags: { channel, status: 'failed' },
        },
      ]);
    }
  }

  async sendEventAlert(
    eventType: string,
    message: string,
    recipients: NotificationRecipient[]
  ): Promise<void> {
    await this.send(
      {
        subject: `Event Alert: ${eventType}`,
        message,
        metadata: { eventType, alertedAt: new Date().toISOString() },
      },
      recipients
    );
  }

  async sendSystemAlert(message: string): Promise<void> {
    const adminRecipients: NotificationRecipient[] = [
      { channel: 'slack', address: '#alerts' },
    ];

    await this.send(
      {
        subject: 'System Alert',
        message,
        metadata: { severity: 'high' },
      },
      adminRecipients
    );
  }
}

export const notificationService = new NotificationService();
"#,
        },
        FileTemplate {
            step: 30,
            path: "src/services/ratelimit.ts",
            purpose: "Rate limiting service",
            content: r#"// Rate limiting service
import { logger } from '../utils/logger';
import { metricsRepository } from '../repos/metrics';

interface RateLimitEntry {
  count: number;
  resetAt: Date;
}

interface RateLimitConfig {
  windowMs: number;
  maxRequests: number;
}

interface RateLimitResult {
  allowed: boolean;
  remaining: number;
  resetAt: Date;
  retryAfterMs?: number;
}

export class RateLimitService {
  private entries: Map<string, RateLimitEntry> = new Map();
  private defaultConfig: RateLimitConfig = {
    windowMs: 60000, // 1 minute
    maxRequests: 100,
  };
  private tierConfigs: Map<string, RateLimitConfig> = new Map();

  constructor() {
    // Set up tier-based limits
    this.tierConfigs.set('free', { windowMs: 60000, maxRequests: 60 });
    this.tierConfigs.set('basic', { windowMs: 60000, maxRequests: 300 });
    this.tierConfigs.set('pro', { windowMs: 60000, maxRequests: 1000 });
    this.tierConfigs.set('enterprise', { windowMs: 60000, maxRequests: 10000 });

    // Clean up expired entries periodically
    setInterval(() => this.cleanup(), 60000);
  }

  check(key: string, tier: string = 'free'): RateLimitResult {
    const config = this.tierConfigs.get(tier) || this.defaultConfig;
    const now = new Date();
    const entry = this.entries.get(key);

    // Check if entry exists and is still valid
    if (entry && entry.resetAt > now) {
      const remaining = Math.max(0, config.maxRequests - entry.count);

      if (entry.count >= config.maxRequests) {
        const retryAfterMs = entry.resetAt.getTime() - now.getTime();

        this.recordRateLimitHit(key, tier);

        return {
          allowed: false,
          remaining: 0,
          resetAt: entry.resetAt,
          retryAfterMs,
        };
      }

      return {
        allowed: true,
        remaining,
        resetAt: entry.resetAt,
      };
    }

    // Create new entry
    const resetAt = new Date(now.getTime() + config.windowMs);
    return {
      allowed: true,
      remaining: config.maxRequests - 1,
      resetAt,
    };
  }

  consume(key: string, tier: string = 'free', tokens: number = 1): RateLimitResult {
    const config = this.tierConfigs.get(tier) || this.defaultConfig;
    const now = new Date();
    let entry = this.entries.get(key);

    // Reset if window expired
    if (!entry || entry.resetAt <= now) {
      entry = {
        count: 0,
        resetAt: new Date(now.getTime() + config.windowMs),
      };
      this.entries.set(key, entry);
    }

    // Check if allowed
    if (entry.count + tokens > config.maxRequests) {
      const retryAfterMs = entry.resetAt.getTime() - now.getTime();

      this.recordRateLimitHit(key, tier);

      return {
        allowed: false,
        remaining: 0,
        resetAt: entry.resetAt,
        retryAfterMs,
      };
    }

    // Consume tokens
    entry.count += tokens;
    const remaining = config.maxRequests - entry.count;

    return {
      allowed: true,
      remaining,
      resetAt: entry.resetAt,
    };
  }

  reset(key: string): void {
    this.entries.delete(key);
    logger.debug('Rate limit reset', { key });
  }

  private cleanup(): void {
    const now = new Date();
    let cleaned = 0;

    for (const [key, entry] of this.entries) {
      if (entry.resetAt <= now) {
        this.entries.delete(key);
        cleaned++;
      }
    }

    if (cleaned > 0) {
      logger.debug('Cleaned up rate limit entries', { count: cleaned });
    }
  }

  private async recordRateLimitHit(key: string, tier: string): Promise<void> {
    await metricsRepository.record({
      timestamp: new Date(),
      name: 'ratelimit.exceeded',
      value: 1,
      tags: { tier },
    });
  }

  getKeyForUser(userId: string): string {
    return `user:${userId}`;
  }

  getKeyForIp(ip: string): string {
    return `ip:${ip}`;
  }

  getKeyForEndpoint(userId: string, endpoint: string): string {
    return `endpoint:${userId}:${endpoint}`;
  }
}

export const rateLimitService = new RateLimitService();
"#,
        },
        FileTemplate {
            step: 31,
            path: "src/services/health.ts",
            purpose: "Health check service",
            content: r#"// Health check service
import { db } from '../repos';
import { globalQueue } from '../utils/queue';
import { logger } from '../utils/logger';

export interface HealthStatus {
  status: 'healthy' | 'degraded' | 'unhealthy';
  timestamp: Date;
  version: string;
  uptime: number;
  checks: HealthCheck[];
}

export interface HealthCheck {
  name: string;
  status: 'pass' | 'warn' | 'fail';
  message?: string;
  durationMs: number;
}

export class HealthService {
  private startTime = Date.now();
  private version = process.env.APP_VERSION || '1.0.0';

  async getHealth(): Promise<HealthStatus> {
    const checks = await this.runChecks();
    const status = this.calculateOverallStatus(checks);

    return {
      status,
      timestamp: new Date(),
      version: this.version,
      uptime: (Date.now() - this.startTime) / 1000,
      checks,
    };
  }

  async getReadiness(): Promise<boolean> {
    const health = await this.getHealth();
    return health.status !== 'unhealthy';
  }

  async getLiveness(): Promise<boolean> {
    // Simple liveness check - just verify the process is responsive
    return true;
  }

  private async runChecks(): Promise<HealthCheck[]> {
    const checks: HealthCheck[] = [];

    // Database check
    checks.push(await this.checkDatabase());

    // Queue check
    checks.push(await this.checkQueue());

    // Memory check
    checks.push(this.checkMemory());

    return checks;
  }

  private async checkDatabase(): Promise<HealthCheck> {
    const startTime = Date.now();

    try {
      const stats = db.getStats();
      const durationMs = Date.now() - startTime;

      if (stats.waiting > 10) {
        return {
          name: 'database',
          status: 'warn',
          message: `High connection wait queue: ${stats.waiting}`,
          durationMs,
        };
      }

      return {
        name: 'database',
        status: 'pass',
        message: `Pool: ${stats.inUse}/${stats.total} in use`,
        durationMs,
      };
    } catch (error) {
      return {
        name: 'database',
        status: 'fail',
        message: (error as Error).message,
        durationMs: Date.now() - startTime,
      };
    }
  }

  private async checkQueue(): Promise<HealthCheck> {
    const startTime = Date.now();
    const size = globalQueue.size();
    const processing = globalQueue.processingCount();

    let status: 'pass' | 'warn' | 'fail' = 'pass';
    let message = `Size: ${size}, Processing: ${processing}`;

    if (size > 5000) {
      status = 'warn';
      message = `Queue backlog high: ${size} pending`;
    }

    if (size > 9000) {
      status = 'fail';
      message = `Queue near capacity: ${size}/10000`;
    }

    return {
      name: 'queue',
      status,
      message,
      durationMs: Date.now() - startTime,
    };
  }

  private checkMemory(): HealthCheck {
    const startTime = Date.now();
    const used = process.memoryUsage();
    const heapUsedMB = Math.round(used.heapUsed / 1024 / 1024);
    const heapTotalMB = Math.round(used.heapTotal / 1024 / 1024);
    const heapPercent = (used.heapUsed / used.heapTotal) * 100;

    let status: 'pass' | 'warn' | 'fail' = 'pass';

    if (heapPercent > 80) {
      status = 'warn';
    }
    if (heapPercent > 95) {
      status = 'fail';
    }

    return {
      name: 'memory',
      status,
      message: `Heap: ${heapUsedMB}/${heapTotalMB}MB (${heapPercent.toFixed(1)}%)`,
      durationMs: Date.now() - startTime,
    };
  }

  private calculateOverallStatus(checks: HealthCheck[]): 'healthy' | 'degraded' | 'unhealthy' {
    const hasFail = checks.some(c => c.status === 'fail');
    const hasWarn = checks.some(c => c.status === 'warn');

    if (hasFail) return 'unhealthy';
    if (hasWarn) return 'degraded';
    return 'healthy';
  }
}

export const healthService = new HealthService();
"#,
        },
        FileTemplate {
            step: 32,
            path: "src/services/batch.ts",
            purpose: "Batch processing service",
            content: r#"// Batch processing service
import { eventService } from './events';
import { eventRepository } from '../repos/events';
import { metricsRepository } from '../repos/metrics';
import { Event, EventPayload } from '../types/events';
import { Result } from '../types/common';
import { logger } from '../utils/logger';

export interface BatchResult {
  total: number;
  succeeded: number;
  failed: number;
  events: Array<{ id: string; success: boolean; error?: string }>;
}

export interface BatchOptions {
  maxBatchSize?: number;
  continueOnError?: boolean;
  validateBeforeProcess?: boolean;
}

const defaultOptions: BatchOptions = {
  maxBatchSize: 1000,
  continueOnError: true,
  validateBeforeProcess: true,
};

export class BatchService {
  async createBatch(
    payloads: EventPayload[],
    userId: string,
    options: BatchOptions = {}
  ): Promise<Result<BatchResult>> {
    const opts = { ...defaultOptions, ...options };
    const startTime = Date.now();

    // Validate batch size
    if (payloads.length > opts.maxBatchSize!) {
      return {
        success: false,
        error: new Error(`Batch size ${payloads.length} exceeds maximum ${opts.maxBatchSize}`),
      };
    }

    // Validate payloads if required
    if (opts.validateBeforeProcess) {
      const validationErrors = this.validatePayloads(payloads);
      if (validationErrors.length > 0) {
        return {
          success: false,
          error: new Error(`Validation failed: ${validationErrors.join(', ')}`),
        };
      }
    }

    const result: BatchResult = {
      total: payloads.length,
      succeeded: 0,
      failed: 0,
      events: [],
    };

    for (const payload of payloads) {
      const createResult = await eventService.createEvent(payload, userId);

      if (createResult.success && createResult.data) {
        result.succeeded++;
        result.events.push({ id: createResult.data.id, success: true });
      } else {
        result.failed++;
        result.events.push({
          id: '',
          success: false,
          error: createResult.error?.message,
        });

        if (!opts.continueOnError) {
          break;
        }
      }
    }

    const durationMs = Date.now() - startTime;
    await this.recordBatchMetrics(result, durationMs);

    logger.info('Batch processing completed', {
      total: result.total,
      succeeded: result.succeeded,
      failed: result.failed,
      durationMs,
    });

    return { success: true, data: result };
  }

  async processBatch(
    filter: { status?: string; type?: string },
    processor: (event: Event) => Promise<void>,
    options: BatchOptions = {}
  ): Promise<BatchResult> {
    const opts = { ...defaultOptions, ...options };
    const events = await eventRepository.findByFilter(filter as any, {
      limit: opts.maxBatchSize,
    });

    const result: BatchResult = {
      total: events.length,
      succeeded: 0,
      failed: 0,
      events: [],
    };

    for (const event of events) {
      try {
        await processor(event);
        result.succeeded++;
        result.events.push({ id: event.id, success: true });
      } catch (error) {
        result.failed++;
        result.events.push({
          id: event.id,
          success: false,
          error: (error as Error).message,
        });

        if (!opts.continueOnError) {
          break;
        }
      }
    }

    return result;
  }

  async retryFailed(userId: string, limit: number = 100): Promise<BatchResult> {
    const failedEvents = await eventRepository.findByFilter(
      { status: 'failed' },
      { limit }
    );

    const result: BatchResult = {
      total: failedEvents.length,
      succeeded: 0,
      failed: 0,
      events: [],
    };

    for (const event of failedEvents) {
      if (event.retryCount < event.maxRetries) {
        const updateResult = await eventService.updateStatus(
          event.id,
          'pending',
          userId
        );

        if (updateResult.success) {
          result.succeeded++;
          result.events.push({ id: event.id, success: true });
        } else {
          result.failed++;
          result.events.push({
            id: event.id,
            success: false,
            error: 'Max retries exceeded',
          });
        }
      }
    }

    logger.info('Retry batch completed', result);
    return result;
  }

  private validatePayloads(payloads: EventPayload[]): string[] {
    const errors: string[] = [];

    for (let i = 0; i < payloads.length; i++) {
      const payload = payloads[i];

      if (!payload.type) {
        errors.push(`Payload[${i}]: missing type`);
      }
      if (!payload.data || typeof payload.data !== 'object') {
        errors.push(`Payload[${i}]: invalid data`);
      }
    }

    return errors;
  }

  private async recordBatchMetrics(result: BatchResult, durationMs: number): Promise<void> {
    await metricsRepository.recordBatch([
      {
        timestamp: new Date(),
        name: 'batch.processed',
        value: result.total,
        tags: {},
      },
      {
        timestamp: new Date(),
        name: 'batch.succeeded',
        value: result.succeeded,
        tags: {},
      },
      {
        timestamp: new Date(),
        name: 'batch.failed',
        value: result.failed,
        tags: {},
      },
      {
        timestamp: new Date(),
        name: 'batch.duration_ms',
        value: durationMs,
        tags: {},
      },
    ]);
  }
}

export const batchService = new BatchService();
"#,
        },
        FileTemplate {
            step: 33,
            path: "src/services/search.ts",
            purpose: "Search service",
            content: r#"// Search service
import { eventRepository } from '../repos/events';
import { userRepository } from '../repos/users';
import { Event } from '../types/events';
import { User } from '../types/users';
import { PaginatedResult } from '../types/common';
import { logger } from '../utils/logger';

export interface SearchQuery {
  q: string;
  type?: 'events' | 'users' | 'all';
  filters?: Record<string, string>;
  page?: number;
  pageSize?: number;
}

export interface SearchResult {
  events?: PaginatedResult<Event>;
  users?: PaginatedResult<User>;
  totalResults: number;
  queryTime: number;
}

export class SearchService {
  async search(query: SearchQuery): Promise<SearchResult> {
    const startTime = Date.now();
    const type = query.type || 'all';
    const page = query.page || 1;
    const pageSize = query.pageSize || 20;

    const result: SearchResult = {
      totalResults: 0,
      queryTime: 0,
    };

    if (type === 'events' || type === 'all') {
      result.events = await this.searchEvents(query.q, query.filters, page, pageSize);
      result.totalResults += result.events.total;
    }

    if (type === 'users' || type === 'all') {
      result.users = await this.searchUsers(query.q, query.filters, page, pageSize);
      result.totalResults += result.users.total;
    }

    result.queryTime = Date.now() - startTime;

    logger.debug('Search completed', {
      query: query.q,
      type,
      totalResults: result.totalResults,
      queryTime: result.queryTime,
    });

    return result;
  }

  private async searchEvents(
    q: string,
    filters: Record<string, string> | undefined,
    page: number,
    pageSize: number
  ): Promise<PaginatedResult<Event>> {
    // Build filter from search query
    const eventFilter: any = {};

    if (filters?.status) {
      eventFilter.status = filters.status;
    }
    if (filters?.type) {
      eventFilter.type = filters.type;
    }

    const offset = (page - 1) * pageSize;
    const events = await eventRepository.findByFilter(eventFilter, {
      limit: pageSize,
      offset,
    });

    // Filter by search query (in real impl, would use full-text search)
    const filtered = events.filter(event =>
      this.matchesQuery(event, q)
    );

    const total = await eventRepository.count();

    return {
      items: filtered,
      total,
      page,
      pageSize,
      hasMore: offset + filtered.length < total,
    };
  }

  private async searchUsers(
    q: string,
    filters: Record<string, string> | undefined,
    page: number,
    pageSize: number
  ): Promise<PaginatedResult<User>> {
    const offset = (page - 1) * pageSize;
    const users = await userRepository.findAll({
      limit: pageSize,
      offset,
    });

    // Filter by search query
    const filtered = users.filter(user =>
      user.email.includes(q) || user.name.toLowerCase().includes(q.toLowerCase())
    );

    // Apply role filter if present
    const withRoleFilter = filters?.role
      ? filtered.filter(u => u.role === filters.role)
      : filtered;

    const total = await userRepository.count();

    return {
      items: withRoleFilter,
      total,
      page,
      pageSize,
      hasMore: offset + withRoleFilter.length < total,
    };
  }

  private matchesQuery(event: Event, query: string): boolean {
    const searchableText = [
      event.id,
      event.payload.type,
      JSON.stringify(event.payload.data),
    ].join(' ').toLowerCase();

    return searchableText.includes(query.toLowerCase());
  }

  async suggest(prefix: string, type: 'events' | 'users'): Promise<string[]> {
    // Return suggestions based on prefix
    const suggestions: string[] = [];

    if (type === 'events') {
      // Suggest event types
      const commonTypes = ['order.created', 'user.signup', 'payment.completed'];
      suggestions.push(...commonTypes.filter(t => t.startsWith(prefix)));
    }

    if (type === 'users') {
      // Suggest user emails (in real impl, would query DB)
      suggestions.push(`${prefix}@example.com`);
    }

    return suggestions.slice(0, 10);
  }
}

export const searchService = new SearchService();
"#,
        },
        FileTemplate {
            step: 34,
            path: "src/services/export.ts",
            purpose: "Data export service",
            content: r#"// Data export service
import { eventRepository } from '../repos/events';
import { auditRepository } from '../repos/audit';
import { metricsRepository } from '../repos/metrics';
import { EventFilter } from '../types/events';
import { logger } from '../utils/logger';

export type ExportFormat = 'json' | 'csv' | 'ndjson';

export interface ExportOptions {
  format: ExportFormat;
  includeMetadata?: boolean;
  dateRange?: { from: Date; to: Date };
}

export interface ExportResult {
  data: string;
  format: ExportFormat;
  recordCount: number;
  exportedAt: Date;
}

export class ExportService {
  async exportEvents(
    filter: EventFilter,
    options: ExportOptions
  ): Promise<ExportResult> {
    const events = await eventRepository.findByFilter(filter, { limit: 10000 });

    logger.info('Exporting events', {
      count: events.length,
      format: options.format,
    });

    const data = this.formatData(events, options);

    return {
      data,
      format: options.format,
      recordCount: events.length,
      exportedAt: new Date(),
    };
  }

  async exportAuditLog(
    filter: { userId?: string; from?: Date; to?: Date },
    options: ExportOptions
  ): Promise<ExportResult> {
    const entries = await auditRepository.find({
      userId: filter.userId,
      from: filter.from,
      to: filter.to,
    }, 10000);

    const data = this.formatData(entries, options);

    return {
      data,
      format: options.format,
      recordCount: entries.length,
      exportedAt: new Date(),
    };
  }

  async exportMetrics(
    metricName: string,
    dateRange: { from: Date; to: Date },
    options: ExportOptions
  ): Promise<ExportResult> {
    const timeSeries = await metricsRepository.getTimeSeries(
      metricName,
      dateRange.from,
      dateRange.to
    );

    const data = this.formatData(timeSeries, options);

    return {
      data,
      format: options.format,
      recordCount: timeSeries.length,
      exportedAt: new Date(),
    };
  }

  private formatData(records: unknown[], options: ExportOptions): string {
    switch (options.format) {
      case 'json':
        return this.toJson(records, options.includeMetadata);
      case 'csv':
        return this.toCsv(records);
      case 'ndjson':
        return this.toNdjson(records);
      default:
        throw new Error(`Unsupported format: ${options.format}`);
    }
  }

  private toJson(records: unknown[], includeMetadata?: boolean): string {
    if (includeMetadata) {
      return JSON.stringify({
        exportedAt: new Date().toISOString(),
        count: records.length,
        data: records,
      }, null, 2);
    }
    return JSON.stringify(records, null, 2);
  }

  private toCsv(records: unknown[]): string {
    if (records.length === 0) {
      return '';
    }

    const firstRecord = records[0] as Record<string, unknown>;
    const headers = Object.keys(firstRecord);
    const lines: string[] = [headers.join(',')];

    for (const record of records) {
      const values = headers.map(h => {
        const value = (record as Record<string, unknown>)[h];
        return this.escapeCsvValue(value);
      });
      lines.push(values.join(','));
    }

    return lines.join('\n');
  }

  private escapeCsvValue(value: unknown): string {
    if (value === null || value === undefined) {
      return '';
    }

    const str = typeof value === 'object' ? JSON.stringify(value) : String(value);

    if (str.includes(',') || str.includes('"') || str.includes('\n')) {
      return `"${str.replace(/"/g, '""')}"`;
    }

    return str;
  }

  private toNdjson(records: unknown[]): string {
    return records.map(r => JSON.stringify(r)).join('\n');
  }
}

export const exportService = new ExportService();
"#,
        },
        FileTemplate {
            step: 35,
            path: "src/services/index.ts",
            purpose: "Service layer exports",
            content: r#"// Service layer exports
export { authService, AuthService, AuthResult } from './auth';
export { eventService, EventService } from './events';
export { userService, UserService } from './users';
export { webhookService, WebhookService } from './webhooks';
export { subscriptionService, SubscriptionService } from './subscriptions';
export { metricsService, MetricsService, DashboardMetrics, EventMetrics, UserMetrics, SystemMetrics } from './metrics';
export { eventProcessor, EventProcessor } from './processor';
export { scheduler, EventScheduler } from './scheduler';
export { notificationService, NotificationService, NotificationChannel, NotificationPayload } from './notifications';
export { rateLimitService, RateLimitService } from './ratelimit';
export { healthService, HealthService, HealthStatus, HealthCheck } from './health';
export { batchService, BatchService, BatchResult, BatchOptions } from './batch';
export { searchService, SearchService, SearchQuery, SearchResult } from './search';
export { exportService, ExportService, ExportFormat, ExportOptions, ExportResult } from './export';
"#,
        },

        // ============================================================
        // PHASE 4: Handlers (Steps 36-50)
        // ============================================================
        FileTemplate {
            step: 36,
            path: "src/handlers/context.ts",
            purpose: "Request context handling",
            content: r#"// Request context handling
import { User } from '../types/users';

export interface RequestContext {
  requestId: string;
  user?: User;
  startTime: Date;
  path: string;
  method: string;
  ip: string;
  userAgent?: string;
}

export function createContext(req: {
  path: string;
  method: string;
  ip?: string;
  headers?: Record<string, string>;
}): RequestContext {
  return {
    requestId: generateRequestId(),
    startTime: new Date(),
    path: req.path,
    method: req.method,
    ip: req.ip || '127.0.0.1',
    userAgent: req.headers?.['user-agent'],
  };
}

export function withUser(ctx: RequestContext, user: User): RequestContext {
  return { ...ctx, user };
}

export function getElapsedMs(ctx: RequestContext): number {
  return Date.now() - ctx.startTime.getTime();
}

function generateRequestId(): string {
  return `req_${Date.now().toString(36)}_${Math.random().toString(36).substr(2, 9)}`;
}

export function extractToken(headers: Record<string, string>): string | null {
  const auth = headers['authorization'];
  if (!auth) return null;

  if (auth.startsWith('Bearer ')) {
    return auth.substring(7);
  }
  return null;
}

export function extractApiKey(headers: Record<string, string>): string | null {
  return headers['x-api-key'] || null;
}
"#,
        },
        FileTemplate {
            step: 37,
            path: "src/handlers/response.ts",
            purpose: "Response formatting utilities",
            content: r#"// Response formatting utilities
import { Result, PaginatedResult } from '../types/common';
import { RequestContext, getElapsedMs } from './context';

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: ApiError;
  meta: ResponseMeta;
}

export interface ApiError {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}

export interface ResponseMeta {
  requestId: string;
  timestamp: string;
  durationMs: number;
}

export interface PaginatedResponse<T> extends ApiResponse<T[]> {
  pagination: {
    page: number;
    pageSize: number;
    total: number;
    hasMore: boolean;
  };
}

export function success<T>(ctx: RequestContext, data: T): ApiResponse<T> {
  return {
    success: true,
    data,
    meta: buildMeta(ctx),
  };
}

export function error(
  ctx: RequestContext,
  code: string,
  message: string,
  details?: Record<string, unknown>
): ApiResponse<never> {
  return {
    success: false,
    error: { code, message, details },
    meta: buildMeta(ctx),
  };
}

export function fromResult<T>(
  ctx: RequestContext,
  result: Result<T>
): ApiResponse<T> {
  if (result.success && result.data !== undefined) {
    return success(ctx, result.data);
  }

  return error(
    ctx,
    'OPERATION_FAILED',
    result.error?.message || 'Unknown error'
  );
}

export function paginated<T>(
  ctx: RequestContext,
  result: PaginatedResult<T>
): PaginatedResponse<T> {
  return {
    success: true,
    data: result.items,
    pagination: {
      page: result.page,
      pageSize: result.pageSize,
      total: result.total,
      hasMore: result.hasMore,
    },
    meta: buildMeta(ctx),
  };
}

function buildMeta(ctx: RequestContext): ResponseMeta {
  return {
    requestId: ctx.requestId,
    timestamp: new Date().toISOString(),
    durationMs: getElapsedMs(ctx),
  };
}

export function notFound(ctx: RequestContext, resource: string): ApiResponse<never> {
  return error(ctx, 'NOT_FOUND', `${resource} not found`);
}

export function unauthorized(ctx: RequestContext): ApiResponse<never> {
  return error(ctx, 'UNAUTHORIZED', 'Authentication required');
}

export function forbidden(ctx: RequestContext): ApiResponse<never> {
  return error(ctx, 'FORBIDDEN', 'Access denied');
}

export function badRequest(ctx: RequestContext, message: string): ApiResponse<never> {
  return error(ctx, 'BAD_REQUEST', message);
}

export function rateLimited(ctx: RequestContext, retryAfterMs: number): ApiResponse<never> {
  return error(ctx, 'RATE_LIMITED', 'Too many requests', { retryAfterMs });
}
"#,
        },
        FileTemplate {
            step: 38,
            path: "src/handlers/auth.ts",
            purpose: "Authentication handlers",
            content: r#"// Authentication handlers
import { authService } from '../services/auth';
import { RequestContext, createContext, withUser, extractToken } from './context';
import { ApiResponse, success, error, unauthorized } from './response';
import { UserCredentials, UserSession, User } from '../types/users';
import { rateLimitService } from '../services/ratelimit';
import { metricsService } from '../services/metrics';
import { logger } from '../utils/logger';

export interface LoginRequest {
  email: string;
  password: string;
}

export interface LoginResponse {
  user: User;
  session: UserSession;
}

export async function handleLogin(
  ctx: RequestContext,
  request: LoginRequest
): Promise<ApiResponse<LoginResponse>> {
  // Rate limit login attempts
  const rateLimitKey = rateLimitService.getKeyForIp(ctx.ip);
  const rateLimit = rateLimitService.consume(rateLimitKey, 'free', 1);

  if (!rateLimit.allowed) {
    logger.warn('Login rate limited', { ip: ctx.ip });
    return error(ctx, 'RATE_LIMITED', 'Too many login attempts', {
      retryAfterMs: rateLimit.retryAfterMs,
    });
  }

  const credentials: UserCredentials = {
    email: request.email,
    password: request.password,
  };

  const result = await authService.login(credentials);

  if (!result.success || !result.data) {
    await metricsService.recordRequestMetric(ctx.path, ctx.method, 401, 0);
    return error(ctx, 'INVALID_CREDENTIALS', 'Invalid email or password');
  }

  await metricsService.recordRequestMetric(ctx.path, ctx.method, 200, 0);

  return success(ctx, {
    user: result.data.user,
    session: result.data.session,
  });
}

export async function handleLogout(
  ctx: RequestContext,
  headers: Record<string, string>
): Promise<ApiResponse<{ success: boolean }>> {
  const token = extractToken(headers);

  if (!token) {
    return unauthorized(ctx);
  }

  await authService.logout(token);
  return success(ctx, { success: true });
}

export async function handleValidateSession(
  ctx: RequestContext,
  headers: Record<string, string>
): Promise<ApiResponse<User>> {
  const token = extractToken(headers);

  if (!token) {
    return unauthorized(ctx);
  }

  const user = await authService.validateSession(token);

  if (!user) {
    return unauthorized(ctx);
  }

  return success(ctx, user);
}

export async function requireAuth(
  ctx: RequestContext,
  headers: Record<string, string>
): Promise<RequestContext | null> {
  const token = extractToken(headers);

  if (!token) {
    return null;
  }

  const user = await authService.validateSession(token);

  if (!user) {
    return null;
  }

  return withUser(ctx, user);
}

export async function requireApiKey(
  ctx: RequestContext,
  headers: Record<string, string>
): Promise<RequestContext | null> {
  const apiKey = headers['x-api-key'];

  if (!apiKey) {
    return null;
  }

  const user = await authService.validateApiKey(apiKey);

  if (!user) {
    return null;
  }

  return withUser(ctx, user);
}

export function requireRole(user: User, ...roles: string[]): boolean {
  return roles.includes(user.role);
}
"#,
        },
        FileTemplate {
            step: 39,
            path: "src/handlers/events.ts",
            purpose: "Event handlers",
            content: r#"// Event handlers
import { eventService } from '../services/events';
import { RequestContext } from './context';
import { ApiResponse, success, fromResult, paginated, notFound, badRequest } from './response';
import { Event, EventPayload, EventFilter, EventStatus } from '../types/events';
import { PaginatedResult } from '../types/common';
import { isValidUUID } from '../utils/validation';
import { logger } from '../utils/logger';

export interface CreateEventRequest {
  type: string;
  data: Record<string, unknown>;
  metadata?: Record<string, string>;
  maxRetries?: number;
  scheduledFor?: string;
}

export interface UpdateEventStatusRequest {
  status: EventStatus;
}

export interface ListEventsQuery {
  status?: EventStatus;
  type?: string;
  from?: string;
  to?: string;
  page?: number;
  pageSize?: number;
}

export async function handleCreateEvent(
  ctx: RequestContext,
  request: CreateEventRequest
): Promise<ApiResponse<Event>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Validate request
  if (!request.type || !request.type.trim()) {
    return badRequest(ctx, 'Event type is required');
  }

  if (!request.data || typeof request.data !== 'object') {
    return badRequest(ctx, 'Event data must be an object');
  }

  const payload: EventPayload = {
    type: request.type,
    data: request.data,
    metadata: request.metadata,
  };

  const options = {
    maxRetries: request.maxRetries,
    scheduledFor: request.scheduledFor ? new Date(request.scheduledFor) : undefined,
  };

  const result = await eventService.createEvent(payload, ctx.user.id, options);
  return fromResult(ctx, result);
}

export async function handleGetEvent(
  ctx: RequestContext,
  eventId: string
): Promise<ApiResponse<Event>> {
  if (!isValidUUID(eventId)) {
    return badRequest(ctx, 'Invalid event ID format');
  }

  const event = await eventService.getEvent(eventId);

  if (!event) {
    return notFound(ctx, 'Event');
  }

  return success(ctx, event);
}

export async function handleListEvents(
  ctx: RequestContext,
  query: ListEventsQuery
): Promise<ApiResponse<Event[]>> {
  const filter: EventFilter = {};

  if (query.status) {
    filter.status = query.status;
  }
  if (query.type) {
    filter.type = query.type;
  }
  if (query.from) {
    filter.fromDate = new Date(query.from);
  }
  if (query.to) {
    filter.toDate = new Date(query.to);
  }

  const page = query.page || 1;
  const pageSize = Math.min(query.pageSize || 20, 100);

  const result = await eventService.listEvents(filter, page, pageSize);
  return paginated(ctx, result);
}

export async function handleUpdateEventStatus(
  ctx: RequestContext,
  eventId: string,
  request: UpdateEventStatusRequest
): Promise<ApiResponse<Event>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  if (!isValidUUID(eventId)) {
    return badRequest(ctx, 'Invalid event ID format');
  }

  const validStatuses: EventStatus[] = ['pending', 'processing', 'completed', 'failed'];
  if (!validStatuses.includes(request.status)) {
    return badRequest(ctx, `Invalid status. Must be one of: ${validStatuses.join(', ')}`);
  }

  const result = await eventService.updateStatus(eventId, request.status, ctx.user.id);
  return fromResult(ctx, result);
}

export async function handleCancelEvent(
  ctx: RequestContext,
  eventId: string
): Promise<ApiResponse<Event>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  const result = await eventService.updateStatus(eventId, 'failed', ctx.user.id);
  return fromResult(ctx, result);
}
"#,
        },
        FileTemplate {
            step: 40,
            path: "src/handlers/users.ts",
            purpose: "User handlers",
            content: r#"// User handlers
import { userService } from '../services/users';
import { RequestContext } from './context';
import { ApiResponse, success, fromResult, paginated, notFound, badRequest, forbidden } from './response';
import { User, CreateUserRequest, UserRole } from '../types/users';
import { isValidEmail, isValidUUID } from '../utils/validation';
import { logger } from '../utils/logger';

export interface CreateUserRequestBody {
  email: string;
  name: string;
  password: string;
  role?: UserRole;
}

export interface UpdateUserRoleRequest {
  role: UserRole;
}

export interface ListUsersQuery {
  page?: number;
  pageSize?: number;
  role?: UserRole;
}

export async function handleCreateUser(
  ctx: RequestContext,
  request: CreateUserRequestBody
): Promise<ApiResponse<User>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Only admins can create users
  if (ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  // Validate request
  if (!isValidEmail(request.email)) {
    return badRequest(ctx, 'Invalid email format');
  }

  if (!request.name || request.name.trim().length < 2) {
    return badRequest(ctx, 'Name must be at least 2 characters');
  }

  if (!request.password || request.password.length < 8) {
    return badRequest(ctx, 'Password must be at least 8 characters');
  }

  const createRequest: CreateUserRequest = {
    email: request.email.toLowerCase().trim(),
    name: request.name.trim(),
    password: request.password,
    role: request.role || 'viewer',
  };

  const result = await userService.createUser(createRequest, ctx.user.id);
  return fromResult(ctx, result);
}

export async function handleGetUser(
  ctx: RequestContext,
  userId: string
): Promise<ApiResponse<User>> {
  if (!isValidUUID(userId)) {
    return badRequest(ctx, 'Invalid user ID format');
  }

  const user = await userService.getUser(userId);

  if (!user) {
    return notFound(ctx, 'User');
  }

  return success(ctx, user);
}

export async function handleGetCurrentUser(
  ctx: RequestContext
): Promise<ApiResponse<User>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  return success(ctx, ctx.user);
}

export async function handleListUsers(
  ctx: RequestContext,
  query: ListUsersQuery
): Promise<ApiResponse<User[]>> {
  if (!ctx.user || ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  const page = query.page || 1;
  const pageSize = Math.min(query.pageSize || 20, 100);

  const result = await userService.listUsers(page, pageSize);
  return paginated(ctx, result);
}

export async function handleUpdateUserRole(
  ctx: RequestContext,
  userId: string,
  request: UpdateUserRoleRequest
): Promise<ApiResponse<User>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Only admins can update roles
  if (ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  // Cannot modify own role
  if (userId === ctx.user.id) {
    return badRequest(ctx, 'Cannot modify own role');
  }

  const validRoles: UserRole[] = ['admin', 'operator', 'viewer'];
  if (!validRoles.includes(request.role)) {
    return badRequest(ctx, `Invalid role. Must be one of: ${validRoles.join(', ')}`);
  }

  const result = await userService.updateRole(userId, request.role, ctx.user.id);
  return fromResult(ctx, result);
}

export async function handleRegenerateApiKey(
  ctx: RequestContext,
  userId: string
): Promise<ApiResponse<{ apiKey: string }>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Can only regenerate own key or admin can do any
  if (userId !== ctx.user.id && ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  const result = await userService.regenerateApiKey(userId, ctx.user.id);

  if (!result.success || !result.data) {
    return fromResult(ctx, result as any);
  }

  return success(ctx, { apiKey: result.data });
}

export async function handleDeleteUser(
  ctx: RequestContext,
  userId: string
): Promise<ApiResponse<{ success: boolean }>> {
  if (!ctx.user || ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  if (userId === ctx.user.id) {
    return badRequest(ctx, 'Cannot delete own account');
  }

  const result = await userService.deleteUser(userId, ctx.user.id);

  if (!result.success) {
    return fromResult(ctx, result);
  }

  return success(ctx, { success: true });
}
"#,
        },
        FileTemplate {
            step: 41,
            path: "src/handlers/webhooks.ts",
            purpose: "Webhook handlers",
            content: r#"// Webhook handlers
import { webhookService } from '../services/webhooks';
import { RequestContext } from './context';
import { ApiResponse, success, fromResult, notFound, badRequest, forbidden } from './response';
import { Webhook, CreateWebhookRequest } from '../repos/webhooks';
import { isValidUUID } from '../utils/validation';
import { logger } from '../utils/logger';

export interface CreateWebhookRequestBody {
  url: string;
  eventTypes: string[];
}

export interface UpdateWebhookRequest {
  eventTypes?: string[];
  isActive?: boolean;
}

export async function handleCreateWebhook(
  ctx: RequestContext,
  request: CreateWebhookRequestBody
): Promise<ApiResponse<Webhook>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Validate URL
  try {
    const url = new URL(request.url);
    if (url.protocol !== 'https:' && url.protocol !== 'http:') {
      return badRequest(ctx, 'Webhook URL must use HTTP or HTTPS');
    }
  } catch {
    return badRequest(ctx, 'Invalid webhook URL');
  }

  // Validate event types
  if (!request.eventTypes || request.eventTypes.length === 0) {
    return badRequest(ctx, 'At least one event type is required');
  }

  const createRequest: CreateWebhookRequest = {
    url: request.url,
    eventTypes: request.eventTypes,
    userId: ctx.user.id,
  };

  const result = await webhookService.createWebhook(createRequest, ctx.user.id);
  return fromResult(ctx, result);
}

export async function handleListWebhooks(
  ctx: RequestContext
): Promise<ApiResponse<Webhook[]>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  const webhooks = await webhookService.getUserWebhooks(ctx.user.id);
  return success(ctx, webhooks);
}

export async function handleUpdateWebhook(
  ctx: RequestContext,
  webhookId: string,
  request: UpdateWebhookRequest
): Promise<ApiResponse<Webhook>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  if (!isValidUUID(webhookId)) {
    return badRequest(ctx, 'Invalid webhook ID format');
  }

  // Verify ownership
  const webhooks = await webhookService.getUserWebhooks(ctx.user.id);
  const webhook = webhooks.find(w => w.id === webhookId);

  if (!webhook) {
    return notFound(ctx, 'Webhook');
  }

  if (request.isActive !== undefined) {
    const result = await webhookService.toggleWebhook(webhookId, request.isActive, ctx.user.id);
    return fromResult(ctx, result);
  }

  // Handle event types update through repository directly
  return success(ctx, webhook);
}

export async function handleDeleteWebhook(
  ctx: RequestContext,
  webhookId: string
): Promise<ApiResponse<{ success: boolean }>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Verify ownership
  const webhooks = await webhookService.getUserWebhooks(ctx.user.id);
  const webhook = webhooks.find(w => w.id === webhookId);

  if (!webhook) {
    return notFound(ctx, 'Webhook');
  }

  // Disable instead of delete (soft delete)
  await webhookService.toggleWebhook(webhookId, false, ctx.user.id);
  return success(ctx, { success: true });
}

export async function handleTestWebhook(
  ctx: RequestContext,
  webhookId: string
): Promise<ApiResponse<{ delivered: boolean }>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Verify ownership
  const webhooks = await webhookService.getUserWebhooks(ctx.user.id);
  const webhook = webhooks.find(w => w.id === webhookId);

  if (!webhook) {
    return notFound(ctx, 'Webhook');
  }

  // Create a test event
  const testEvent = {
    id: 'test_' + Date.now(),
    payload: {
      type: 'webhook.test',
      data: { message: 'This is a test webhook delivery' },
    },
    status: 'completed' as const,
    retryCount: 0,
    maxRetries: 0,
    createdAt: new Date(),
    updatedAt: new Date(),
  };

  const results = await webhookService.deliverEvent(testEvent);
  const delivered = results.some(r => r.webhookId === webhookId && r.success);

  return success(ctx, { delivered });
}
"#,
        },
        FileTemplate {
            step: 42,
            path: "src/handlers/subscriptions.ts",
            purpose: "Subscription handlers",
            content: r#"// Subscription handlers
import { subscriptionService } from '../services/subscriptions';
import { RequestContext } from './context';
import { ApiResponse, success, fromResult, notFound, badRequest } from './response';
import { Subscription, CreateSubscriptionRequest } from '../repos/subscriptions';
import { isValidUUID } from '../utils/validation';
import { logger } from '../utils/logger';

export interface CreateSubscriptionBody {
  eventPattern: string;
  callbackUrl: string;
}

export async function handleCreateSubscription(
  ctx: RequestContext,
  request: CreateSubscriptionBody
): Promise<ApiResponse<Subscription>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Validate pattern
  if (!request.eventPattern || !request.eventPattern.trim()) {
    return badRequest(ctx, 'Event pattern is required');
  }

  // Validate callback URL
  try {
    const url = new URL(request.callbackUrl);
    if (url.protocol !== 'https:' && url.protocol !== 'http:') {
      return badRequest(ctx, 'Callback URL must use HTTP or HTTPS');
    }
  } catch {
    return badRequest(ctx, 'Invalid callback URL');
  }

  const createRequest: CreateSubscriptionRequest = {
    userId: ctx.user.id,
    eventPattern: request.eventPattern,
    callbackUrl: request.callbackUrl,
  };

  const result = await subscriptionService.createSubscription(createRequest, ctx.user.id);
  return fromResult(ctx, result);
}

export async function handleListSubscriptions(
  ctx: RequestContext
): Promise<ApiResponse<Subscription[]>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  const subscriptions = await subscriptionService.getUserSubscriptions(ctx.user.id);
  return success(ctx, subscriptions);
}

export async function handleGetSubscription(
  ctx: RequestContext,
  subscriptionId: string
): Promise<ApiResponse<Subscription>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  if (!isValidUUID(subscriptionId)) {
    return badRequest(ctx, 'Invalid subscription ID format');
  }

  const subscriptions = await subscriptionService.getUserSubscriptions(ctx.user.id);
  const subscription = subscriptions.find(s => s.id === subscriptionId);

  if (!subscription) {
    return notFound(ctx, 'Subscription');
  }

  return success(ctx, subscription);
}

export async function handleDeleteSubscription(
  ctx: RequestContext,
  subscriptionId: string
): Promise<ApiResponse<{ success: boolean }>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  if (!isValidUUID(subscriptionId)) {
    return badRequest(ctx, 'Invalid subscription ID format');
  }

  const result = await subscriptionService.deleteSubscription(subscriptionId, ctx.user.id);

  if (!result.success) {
    return fromResult(ctx, result);
  }

  return success(ctx, { success: true });
}
"#,
        },
        FileTemplate {
            step: 43,
            path: "src/handlers/metrics.ts",
            purpose: "Metrics handlers",
            content: r#"// Metrics handlers
import { metricsService, DashboardMetrics } from '../services/metrics';
import { RequestContext } from './context';
import { ApiResponse, success, badRequest, forbidden } from './response';
import { logger } from '../utils/logger';

export interface MetricsQuery {
  metric?: string;
  hours?: number;
}

export async function handleGetDashboard(
  ctx: RequestContext
): Promise<ApiResponse<DashboardMetrics>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Only operators and admins can view dashboard
  if (ctx.user.role === 'viewer') {
    return forbidden(ctx);
  }

  const metrics = await metricsService.getDashboardMetrics();
  return success(ctx, metrics);
}

export async function handleGetTimeSeries(
  ctx: RequestContext,
  query: MetricsQuery
): Promise<ApiResponse<{ timestamp: Date; value: number }[]>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  if (ctx.user.role === 'viewer') {
    return forbidden(ctx);
  }

  if (!query.metric) {
    return badRequest(ctx, 'Metric name is required');
  }

  const hours = query.hours || 24;
  if (hours < 1 || hours > 168) {
    return badRequest(ctx, 'Hours must be between 1 and 168');
  }

  const data = await metricsService.getMetricTimeSeries(query.metric, hours);
  return success(ctx, data);
}

export async function handleGetEventMetrics(
  ctx: RequestContext
): Promise<ApiResponse<{ totalEvents: number; pendingEvents: number; completedToday: number; failedToday: number; avgProcessingTimeMs: number }>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  const metrics = await metricsService.getEventMetrics();
  return success(ctx, metrics);
}

export async function handleGetSystemMetrics(
  ctx: RequestContext
): Promise<ApiResponse<{ queueSize: number; avgLatencyMs: number; errorRate: number }>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  if (ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  const metrics = await metricsService.getSystemMetrics();
  return success(ctx, metrics);
}
"#,
        },
        FileTemplate {
            step: 44,
            path: "src/handlers/health.ts",
            purpose: "Health check handlers",
            content: r#"// Health check handlers
import { healthService, HealthStatus } from '../services/health';
import { RequestContext } from './context';
import { ApiResponse, success } from './response';

export async function handleHealth(
  ctx: RequestContext
): Promise<ApiResponse<HealthStatus>> {
  const health = await healthService.getHealth();
  return success(ctx, health);
}

export async function handleReadiness(
  ctx: RequestContext
): Promise<ApiResponse<{ ready: boolean }>> {
  const ready = await healthService.getReadiness();
  return success(ctx, { ready });
}

export async function handleLiveness(
  ctx: RequestContext
): Promise<ApiResponse<{ alive: boolean }>> {
  const alive = await healthService.getLiveness();
  return success(ctx, { alive });
}

export async function handleVersion(
  ctx: RequestContext
): Promise<ApiResponse<{ version: string; buildDate: string }>> {
  return success(ctx, {
    version: process.env.APP_VERSION || '1.0.0',
    buildDate: process.env.BUILD_DATE || new Date().toISOString(),
  });
}
"#,
        },
        FileTemplate {
            step: 45,
            path: "src/handlers/batch.ts",
            purpose: "Batch operation handlers",
            content: r#"// Batch operation handlers
import { batchService, BatchResult, BatchOptions } from '../services/batch';
import { RequestContext } from './context';
import { ApiResponse, success, fromResult, badRequest, forbidden } from './response';
import { EventPayload } from '../types/events';
import { logger } from '../utils/logger';

export interface BatchCreateRequest {
  events: Array<{
    type: string;
    data: Record<string, unknown>;
    metadata?: Record<string, string>;
  }>;
  options?: BatchOptions;
}

export interface BatchRetryRequest {
  limit?: number;
}

export async function handleBatchCreate(
  ctx: RequestContext,
  request: BatchCreateRequest
): Promise<ApiResponse<BatchResult>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Only operators and admins can create batches
  if (ctx.user.role === 'viewer') {
    return forbidden(ctx);
  }

  // Validate request
  if (!request.events || !Array.isArray(request.events)) {
    return badRequest(ctx, 'Events array is required');
  }

  if (request.events.length === 0) {
    return badRequest(ctx, 'At least one event is required');
  }

  if (request.events.length > 1000) {
    return badRequest(ctx, 'Maximum 1000 events per batch');
  }

  // Convert to EventPayload array
  const payloads: EventPayload[] = request.events.map(e => ({
    type: e.type,
    data: e.data,
    metadata: e.metadata,
  }));

  const result = await batchService.createBatch(payloads, ctx.user.id, request.options);
  return fromResult(ctx, result);
}

export async function handleBatchRetry(
  ctx: RequestContext,
  request: BatchRetryRequest
): Promise<ApiResponse<BatchResult>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Only admins can retry failed events
  if (ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  const limit = Math.min(request.limit || 100, 1000);
  const result = await batchService.retryFailed(ctx.user.id, limit);

  return success(ctx, result);
}

export async function handleBatchStatus(
  ctx: RequestContext
): Promise<ApiResponse<{ pendingBatches: number; processingEvents: number }>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Placeholder - would track actual batch status
  return success(ctx, {
    pendingBatches: 0,
    processingEvents: 0,
  });
}
"#,
        },
        FileTemplate {
            step: 46,
            path: "src/handlers/search.ts",
            purpose: "Search handlers",
            content: r#"// Search handlers
import { searchService, SearchResult, SearchQuery } from '../services/search';
import { RequestContext } from './context';
import { ApiResponse, success, badRequest } from './response';
import { logger } from '../utils/logger';

export interface SearchRequestQuery {
  q: string;
  type?: 'events' | 'users' | 'all';
  status?: string;
  role?: string;
  page?: number;
  pageSize?: number;
}

export interface SuggestRequestQuery {
  prefix: string;
  type: 'events' | 'users';
}

export async function handleSearch(
  ctx: RequestContext,
  query: SearchRequestQuery
): Promise<ApiResponse<SearchResult>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  if (!query.q || query.q.trim().length < 2) {
    return badRequest(ctx, 'Search query must be at least 2 characters');
  }

  const searchQuery: SearchQuery = {
    q: query.q.trim(),
    type: query.type,
    filters: {},
    page: query.page || 1,
    pageSize: Math.min(query.pageSize || 20, 100),
  };

  if (query.status) {
    searchQuery.filters!.status = query.status;
  }
  if (query.role) {
    searchQuery.filters!.role = query.role;
  }

  const result = await searchService.search(searchQuery);
  return success(ctx, result);
}

export async function handleSuggest(
  ctx: RequestContext,
  query: SuggestRequestQuery
): Promise<ApiResponse<string[]>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  if (!query.prefix) {
    return success(ctx, []);
  }

  const suggestions = await searchService.suggest(query.prefix, query.type);
  return success(ctx, suggestions);
}
"#,
        },
        FileTemplate {
            step: 47,
            path: "src/handlers/export.ts",
            purpose: "Export handlers",
            content: r#"// Export handlers
import { exportService, ExportResult, ExportFormat } from '../services/export';
import { RequestContext } from './context';
import { ApiResponse, success, badRequest, forbidden } from './response';
import { EventFilter } from '../types/events';
import { logger } from '../utils/logger';

export interface ExportEventsRequest {
  format: ExportFormat;
  status?: string;
  type?: string;
  from?: string;
  to?: string;
  includeMetadata?: boolean;
}

export interface ExportAuditRequest {
  format: ExportFormat;
  userId?: string;
  from?: string;
  to?: string;
}

export interface ExportMetricsRequest {
  format: ExportFormat;
  metricName: string;
  from: string;
  to: string;
}

export async function handleExportEvents(
  ctx: RequestContext,
  request: ExportEventsRequest
): Promise<ApiResponse<ExportResult>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  if (ctx.user.role === 'viewer') {
    return forbidden(ctx);
  }

  // Validate format
  const validFormats: ExportFormat[] = ['json', 'csv', 'ndjson'];
  if (!validFormats.includes(request.format)) {
    return badRequest(ctx, `Invalid format. Must be one of: ${validFormats.join(', ')}`);
  }

  const filter: EventFilter = {};
  if (request.status) {
    filter.status = request.status as any;
  }
  if (request.type) {
    filter.type = request.type;
  }
  if (request.from) {
    filter.fromDate = new Date(request.from);
  }
  if (request.to) {
    filter.toDate = new Date(request.to);
  }

  const result = await exportService.exportEvents(filter, {
    format: request.format,
    includeMetadata: request.includeMetadata,
  });

  logger.info('Events exported', {
    userId: ctx.user.id,
    format: request.format,
    recordCount: result.recordCount,
  });

  return success(ctx, result);
}

export async function handleExportAudit(
  ctx: RequestContext,
  request: ExportAuditRequest
): Promise<ApiResponse<ExportResult>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  // Only admins can export audit logs
  if (ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  const result = await exportService.exportAuditLog(
    {
      userId: request.userId,
      from: request.from ? new Date(request.from) : undefined,
      to: request.to ? new Date(request.to) : undefined,
    },
    { format: request.format }
  );

  return success(ctx, result);
}

export async function handleExportMetrics(
  ctx: RequestContext,
  request: ExportMetricsRequest
): Promise<ApiResponse<ExportResult>> {
  if (!ctx.user) {
    return badRequest(ctx, 'User context required');
  }

  if (ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  if (!request.metricName) {
    return badRequest(ctx, 'Metric name is required');
  }

  if (!request.from || !request.to) {
    return badRequest(ctx, 'Date range is required');
  }

  const result = await exportService.exportMetrics(
    request.metricName,
    { from: new Date(request.from), to: new Date(request.to) },
    { format: request.format }
  );

  return success(ctx, result);
}
"#,
        },
        FileTemplate {
            step: 48,
            path: "src/handlers/admin.ts",
            purpose: "Admin handlers",
            content: r#"// Admin handlers
import { scheduler } from '../services/scheduler';
import { metricsService } from '../services/metrics';
import { healthService } from '../services/health';
import { RequestContext } from './context';
import { ApiResponse, success, badRequest, forbidden } from './response';
import { logger } from '../utils/logger';

export interface JobActionRequest {
  jobId: string;
  action: 'pause' | 'resume';
}

export async function handleGetJobs(
  ctx: RequestContext
): Promise<ApiResponse<Array<{ id: string; name: string; isActive: boolean; lastRun?: Date; nextRun: Date }>>> {
  if (!ctx.user || ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  const jobs = scheduler.getJobStatus();
  return success(ctx, jobs);
}

export async function handleJobAction(
  ctx: RequestContext,
  request: JobActionRequest
): Promise<ApiResponse<{ success: boolean }>> {
  if (!ctx.user || ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  let result: boolean;

  switch (request.action) {
    case 'pause':
      result = scheduler.pauseJob(request.jobId);
      break;
    case 'resume':
      result = scheduler.resumeJob(request.jobId);
      break;
    default:
      return badRequest(ctx, 'Invalid action');
  }

  if (!result) {
    return badRequest(ctx, 'Job not found');
  }

  logger.info('Job action performed', {
    jobId: request.jobId,
    action: request.action,
    by: ctx.user.id,
  });

  return success(ctx, { success: true });
}

export async function handleCleanupMetrics(
  ctx: RequestContext,
  retentionDays?: number
): Promise<ApiResponse<{ success: boolean }>> {
  if (!ctx.user || ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  const days = retentionDays || 30;
  if (days < 1 || days > 365) {
    return badRequest(ctx, 'Retention days must be between 1 and 365');
  }

  await metricsService.cleanupOldMetrics(days);

  logger.info('Metrics cleanup triggered', {
    retentionDays: days,
    by: ctx.user.id,
  });

  return success(ctx, { success: true });
}

export async function handleSystemStatus(
  ctx: RequestContext
): Promise<ApiResponse<{
  health: { status: string; uptime: number };
  jobs: number;
  version: string;
}>> {
  if (!ctx.user || ctx.user.role !== 'admin') {
    return forbidden(ctx);
  }

  const health = await healthService.getHealth();
  const jobs = scheduler.getJobStatus();

  return success(ctx, {
    health: {
      status: health.status,
      uptime: health.uptime,
    },
    jobs: jobs.length,
    version: process.env.APP_VERSION || '1.0.0',
  });
}
"#,
        },
        FileTemplate {
            step: 49,
            path: "src/handlers/ratelimit.ts",
            purpose: "Rate limit middleware handlers",
            content: r#"// Rate limit middleware handlers
import { rateLimitService } from '../services/ratelimit';
import { RequestContext } from './context';
import { ApiResponse, rateLimited } from './response';
import { logger } from '../utils/logger';

export interface RateLimitOptions {
  tier?: string;
  tokens?: number;
  keyGenerator?: (ctx: RequestContext) => string;
}

const defaultOptions: RateLimitOptions = {
  tier: 'free',
  tokens: 1,
};

export function checkRateLimit(
  ctx: RequestContext,
  options: RateLimitOptions = {}
): ApiResponse<never> | null {
  const opts = { ...defaultOptions, ...options };

  // Generate rate limit key
  const key = opts.keyGenerator
    ? opts.keyGenerator(ctx)
    : getRateLimitKey(ctx);

  // Determine tier from user role if authenticated
  const tier = ctx.user
    ? getTierFromRole(ctx.user.role)
    : opts.tier || 'free';

  const result = rateLimitService.consume(key, tier, opts.tokens);

  if (!result.allowed) {
    logger.warn('Rate limit exceeded', {
      key,
      tier,
      resetAt: result.resetAt,
    });

    return rateLimited(ctx, result.retryAfterMs || 0);
  }

  return null;
}

function getRateLimitKey(ctx: RequestContext): string {
  if (ctx.user) {
    return rateLimitService.getKeyForUser(ctx.user.id);
  }
  return rateLimitService.getKeyForIp(ctx.ip);
}

function getTierFromRole(role: string): string {
  switch (role) {
    case 'admin':
      return 'enterprise';
    case 'operator':
      return 'pro';
    default:
      return 'basic';
  }
}

export function getRateLimitHeaders(ctx: RequestContext): Record<string, string> {
  const key = getRateLimitKey(ctx);
  const tier = ctx.user ? getTierFromRole(ctx.user.role) : 'free';
  const check = rateLimitService.check(key, tier);

  return {
    'X-RateLimit-Limit': getLimit(tier).toString(),
    'X-RateLimit-Remaining': check.remaining.toString(),
    'X-RateLimit-Reset': check.resetAt.toISOString(),
  };
}

function getLimit(tier: string): number {
  const limits: Record<string, number> = {
    free: 60,
    basic: 300,
    pro: 1000,
    enterprise: 10000,
  };
  return limits[tier] || 60;
}

export function createEndpointLimiter(endpoint: string): (ctx: RequestContext) => string {
  return (ctx: RequestContext) => {
    const userId = ctx.user?.id || ctx.ip;
    return rateLimitService.getKeyForEndpoint(userId, endpoint);
  };
}
"#,
        },
        FileTemplate {
            step: 50,
            path: "src/handlers/index.ts",
            purpose: "Handler exports",
            content: r#"// Handler layer exports
export * from './context';
export * from './response';
export * from './auth';
export * from './events';
export * from './users';
export * from './webhooks';
export * from './subscriptions';
export * from './metrics';
export * from './health';
export * from './batch';
export * from './search';
export * from './export';
export * from './admin';
export * from './ratelimit';
"#,
        },

        // ============================================================
        // PHASE 5: Routes & Integration (Steps 51-65)
        // ============================================================
        FileTemplate {
            step: 51,
            path: "src/routes/middleware.ts",
            purpose: "Route middleware",
            content: r#"// Route middleware
import { RequestContext, createContext } from '../handlers/context';
import { requireAuth, requireApiKey } from '../handlers/auth';
import { checkRateLimit, getRateLimitHeaders } from '../handlers/ratelimit';
import { metricsService } from '../services/metrics';
import { logger } from '../utils/logger';

export interface Request {
  path: string;
  method: string;
  headers: Record<string, string>;
  body?: unknown;
  query?: Record<string, string>;
  ip?: string;
}

export interface Response {
  status: number;
  headers: Record<string, string>;
  body: unknown;
}

export type Handler = (ctx: RequestContext, req: Request) => Promise<Response>;
export type Middleware = (handler: Handler) => Handler;

export function withLogging(): Middleware {
  return (handler: Handler) => async (ctx: RequestContext, req: Request) => {
    const startTime = Date.now();

    logger.debug('Request started', {
      requestId: ctx.requestId,
      method: ctx.method,
      path: ctx.path,
    });

    try {
      const response = await handler(ctx, req);

      logger.info('Request completed', {
        requestId: ctx.requestId,
        method: ctx.method,
        path: ctx.path,
        status: response.status,
        durationMs: Date.now() - startTime,
      });

      return response;
    } catch (error) {
      logger.error('Request failed', {
        requestId: ctx.requestId,
        error: (error as Error).message,
        durationMs: Date.now() - startTime,
      });
      throw error;
    }
  };
}

export function withMetrics(): Middleware {
  return (handler: Handler) => async (ctx: RequestContext, req: Request) => {
    const startTime = Date.now();
    const response = await handler(ctx, req);
    const durationMs = Date.now() - startTime;

    await metricsService.recordRequestMetric(
      ctx.path,
      ctx.method,
      response.status,
      durationMs
    );

    return response;
  };
}

export function withAuth(): Middleware {
  return (handler: Handler) => async (ctx: RequestContext, req: Request) => {
    const authCtx = await requireAuth(ctx, req.headers);

    if (!authCtx) {
      return {
        status: 401,
        headers: {},
        body: { error: 'Unauthorized' },
      };
    }

    return handler(authCtx, req);
  };
}

export function withApiKey(): Middleware {
  return (handler: Handler) => async (ctx: RequestContext, req: Request) => {
    const authCtx = await requireApiKey(ctx, req.headers);

    if (!authCtx) {
      return {
        status: 401,
        headers: {},
        body: { error: 'Invalid API key' },
      };
    }

    return handler(authCtx, req);
  };
}

export function withRateLimit(): Middleware {
  return (handler: Handler) => async (ctx: RequestContext, req: Request) => {
    const limited = checkRateLimit(ctx);

    if (limited) {
      return {
        status: 429,
        headers: getRateLimitHeaders(ctx),
        body: limited,
      };
    }

    const response = await handler(ctx, req);
    return {
      ...response,
      headers: { ...response.headers, ...getRateLimitHeaders(ctx) },
    };
  };
}

export function compose(...middlewares: Middleware[]): Middleware {
  return (handler: Handler) => {
    return middlewares.reduceRight((h, m) => m(h), handler);
  };
}
"#,
        },
        FileTemplate {
            step: 52,
            path: "src/routes/auth.ts",
            purpose: "Auth routes",
            content: r#"// Auth routes
import { Request, Response, Handler, compose, withLogging, withMetrics, withRateLimit } from './middleware';
import { RequestContext, createContext } from '../handlers/context';
import { handleLogin, handleLogout, handleValidateSession, LoginRequest } from '../handlers/auth';
import { logger } from '../utils/logger';

export function createAuthRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // POST /auth/login
  routes.set('POST /auth/login', compose(
    withLogging(),
    withMetrics(),
    withRateLimit()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as LoginRequest;
    const result = await handleLogin(ctx, body);

    return {
      status: result.success ? 200 : 401,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // POST /auth/logout
  routes.set('POST /auth/logout', compose(
    withLogging(),
    withMetrics()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleLogout(ctx, req.headers);

    return {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /auth/validate
  routes.set('GET /auth/validate', compose(
    withLogging(),
    withMetrics()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleValidateSession(ctx, req.headers);

    return {
      status: result.success ? 200 : 401,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  return routes;
}
"#,
        },
        FileTemplate {
            step: 53,
            path: "src/routes/events.ts",
            purpose: "Event routes",
            content: r#"// Event routes
import { Request, Response, Handler, compose, withLogging, withMetrics, withRateLimit, withAuth, withApiKey } from './middleware';
import { RequestContext } from '../handlers/context';
import {
  handleCreateEvent,
  handleGetEvent,
  handleListEvents,
  handleUpdateEventStatus,
  handleCancelEvent,
  CreateEventRequest,
  ListEventsQuery,
  UpdateEventStatusRequest,
} from '../handlers/events';

export function createEventRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // POST /events - Create event (API key auth)
  routes.set('POST /events', compose(
    withLogging(),
    withMetrics(),
    withRateLimit(),
    withApiKey()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as CreateEventRequest;
    const result = await handleCreateEvent(ctx, body);

    return {
      status: result.success ? 201 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /events - List events
  routes.set('GET /events', compose(
    withLogging(),
    withMetrics(),
    withRateLimit(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const query = req.query as unknown as ListEventsQuery;
    const result = await handleListEvents(ctx, query);

    return {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /events/:id - Get single event
  routes.set('GET /events/:id', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const eventId = extractPathParam(req.path, '/events/');
    const result = await handleGetEvent(ctx, eventId);

    return {
      status: result.success ? 200 : 404,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // PATCH /events/:id/status - Update event status
  routes.set('PATCH /events/:id/status', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const eventId = extractPathParam(req.path, '/events/', '/status');
    const body = req.body as UpdateEventStatusRequest;
    const result = await handleUpdateEventStatus(ctx, eventId, body);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // DELETE /events/:id - Cancel event
  routes.set('DELETE /events/:id', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const eventId = extractPathParam(req.path, '/events/');
    const result = await handleCancelEvent(ctx, eventId);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  return routes;
}

function extractPathParam(path: string, prefix: string, suffix: string = ''): string {
  let result = path.replace(prefix, '');
  if (suffix) {
    result = result.replace(suffix, '');
  }
  return result;
}
"#,
        },
        FileTemplate {
            step: 54,
            path: "src/routes/users.ts",
            purpose: "User routes",
            content: r#"// User routes
import { Request, Response, Handler, compose, withLogging, withMetrics, withRateLimit, withAuth } from './middleware';
import { RequestContext } from '../handlers/context';
import {
  handleCreateUser,
  handleGetUser,
  handleGetCurrentUser,
  handleListUsers,
  handleUpdateUserRole,
  handleRegenerateApiKey,
  handleDeleteUser,
  CreateUserRequestBody,
  UpdateUserRoleRequest,
  ListUsersQuery,
} from '../handlers/users';

export function createUserRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // GET /users/me - Get current user
  routes.set('GET /users/me', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleGetCurrentUser(ctx);

    return {
      status: result.success ? 200 : 401,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // POST /users - Create user (admin only)
  routes.set('POST /users', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as CreateUserRequestBody;
    const result = await handleCreateUser(ctx, body);

    return {
      status: result.success ? 201 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /users - List users (admin only)
  routes.set('GET /users', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const query = req.query as unknown as ListUsersQuery;
    const result = await handleListUsers(ctx, query);

    return {
      status: result.success ? 200 : 403,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /users/:id - Get user by ID
  routes.set('GET /users/:id', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const userId = extractPathParam(req.path, '/users/');
    const result = await handleGetUser(ctx, userId);

    return {
      status: result.success ? 200 : 404,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // PATCH /users/:id/role - Update user role
  routes.set('PATCH /users/:id/role', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const userId = extractPathParam(req.path, '/users/', '/role');
    const body = req.body as UpdateUserRoleRequest;
    const result = await handleUpdateUserRole(ctx, userId, body);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // POST /users/:id/regenerate-api-key
  routes.set('POST /users/:id/regenerate-api-key', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const userId = extractPathParam(req.path, '/users/', '/regenerate-api-key');
    const result = await handleRegenerateApiKey(ctx, userId);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // DELETE /users/:id - Delete user
  routes.set('DELETE /users/:id', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const userId = extractPathParam(req.path, '/users/');
    const result = await handleDeleteUser(ctx, userId);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  return routes;
}

function extractPathParam(path: string, prefix: string, suffix: string = ''): string {
  let result = path.replace(prefix, '');
  if (suffix) {
    result = result.replace(suffix, '');
  }
  return result;
}
"#,
        },
        FileTemplate {
            step: 55,
            path: "src/routes/webhooks.ts",
            purpose: "Webhook routes",
            content: r#"// Webhook routes
import { Request, Response, Handler, compose, withLogging, withMetrics, withAuth } from './middleware';
import { RequestContext } from '../handlers/context';
import {
  handleCreateWebhook,
  handleListWebhooks,
  handleUpdateWebhook,
  handleDeleteWebhook,
  handleTestWebhook,
  CreateWebhookRequestBody,
  UpdateWebhookRequest,
} from '../handlers/webhooks';

export function createWebhookRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // POST /webhooks
  routes.set('POST /webhooks', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as CreateWebhookRequestBody;
    const result = await handleCreateWebhook(ctx, body);

    return {
      status: result.success ? 201 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /webhooks
  routes.set('GET /webhooks', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleListWebhooks(ctx);

    return {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // PATCH /webhooks/:id
  routes.set('PATCH /webhooks/:id', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const webhookId = extractPathParam(req.path, '/webhooks/');
    const body = req.body as UpdateWebhookRequest;
    const result = await handleUpdateWebhook(ctx, webhookId, body);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // DELETE /webhooks/:id
  routes.set('DELETE /webhooks/:id', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const webhookId = extractPathParam(req.path, '/webhooks/');
    const result = await handleDeleteWebhook(ctx, webhookId);

    return {
      status: result.success ? 200 : 404,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // POST /webhooks/:id/test
  routes.set('POST /webhooks/:id/test', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const webhookId = extractPathParam(req.path, '/webhooks/', '/test');
    const result = await handleTestWebhook(ctx, webhookId);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  return routes;
}

function extractPathParam(path: string, prefix: string, suffix: string = ''): string {
  let result = path.replace(prefix, '');
  if (suffix) {
    result = result.replace(suffix, '');
  }
  return result;
}
"#,
        },
        FileTemplate {
            step: 56,
            path: "src/routes/subscriptions.ts",
            purpose: "Subscription routes",
            content: r#"// Subscription routes
import { Request, Response, Handler, compose, withLogging, withMetrics, withAuth } from './middleware';
import { RequestContext } from '../handlers/context';
import {
  handleCreateSubscription,
  handleListSubscriptions,
  handleGetSubscription,
  handleDeleteSubscription,
  CreateSubscriptionBody,
} from '../handlers/subscriptions';

export function createSubscriptionRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // POST /subscriptions
  routes.set('POST /subscriptions', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as CreateSubscriptionBody;
    const result = await handleCreateSubscription(ctx, body);

    return {
      status: result.success ? 201 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /subscriptions
  routes.set('GET /subscriptions', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleListSubscriptions(ctx);

    return {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /subscriptions/:id
  routes.set('GET /subscriptions/:id', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const subscriptionId = extractPathParam(req.path, '/subscriptions/');
    const result = await handleGetSubscription(ctx, subscriptionId);

    return {
      status: result.success ? 200 : 404,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // DELETE /subscriptions/:id
  routes.set('DELETE /subscriptions/:id', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const subscriptionId = extractPathParam(req.path, '/subscriptions/');
    const result = await handleDeleteSubscription(ctx, subscriptionId);

    return {
      status: result.success ? 200 : 404,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  return routes;
}

function extractPathParam(path: string, prefix: string): string {
  return path.replace(prefix, '');
}
"#,
        },
        FileTemplate {
            step: 57,
            path: "src/routes/metrics.ts",
            purpose: "Metrics routes",
            content: r#"// Metrics routes
import { Request, Response, Handler, compose, withLogging, withMetrics, withAuth } from './middleware';
import { RequestContext } from '../handlers/context';
import {
  handleGetDashboard,
  handleGetTimeSeries,
  handleGetEventMetrics,
  handleGetSystemMetrics,
  MetricsQuery,
} from '../handlers/metrics';

export function createMetricsRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // GET /metrics/dashboard
  routes.set('GET /metrics/dashboard', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleGetDashboard(ctx);

    return {
      status: result.success ? 200 : 403,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /metrics/timeseries
  routes.set('GET /metrics/timeseries', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const query = req.query as unknown as MetricsQuery;
    const result = await handleGetTimeSeries(ctx, query);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /metrics/events
  routes.set('GET /metrics/events', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleGetEventMetrics(ctx);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /metrics/system
  routes.set('GET /metrics/system', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleGetSystemMetrics(ctx);

    return {
      status: result.success ? 200 : 403,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  return routes;
}
"#,
        },
        FileTemplate {
            step: 58,
            path: "src/routes/health.ts",
            purpose: "Health check routes",
            content: r#"// Health check routes
import { Request, Response, Handler, compose, withLogging } from './middleware';
import { RequestContext } from '../handlers/context';
import {
  handleHealth,
  handleReadiness,
  handleLiveness,
  handleVersion,
} from '../handlers/health';

export function createHealthRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // GET /health
  routes.set('GET /health', compose(
    withLogging()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleHealth(ctx);
    const status = result.data?.status === 'unhealthy' ? 503 : 200;

    return {
      status,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /health/ready
  routes.set('GET /health/ready', compose(
    withLogging()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleReadiness(ctx);

    return {
      status: result.data?.ready ? 200 : 503,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /health/live
  routes.set('GET /health/live', compose(
    withLogging()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleLiveness(ctx);

    return {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /version
  routes.set('GET /version', async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleVersion(ctx);

    return {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  });

  return routes;
}
"#,
        },
        FileTemplate {
            step: 59,
            path: "src/routes/batch.ts",
            purpose: "Batch operation routes",
            content: r#"// Batch operation routes
import { Request, Response, Handler, compose, withLogging, withMetrics, withAuth, withRateLimit } from './middleware';
import { RequestContext } from '../handlers/context';
import {
  handleBatchCreate,
  handleBatchRetry,
  handleBatchStatus,
  BatchCreateRequest,
  BatchRetryRequest,
} from '../handlers/batch';

export function createBatchRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // POST /batch/events
  routes.set('POST /batch/events', compose(
    withLogging(),
    withMetrics(),
    withRateLimit(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as BatchCreateRequest;
    const result = await handleBatchCreate(ctx, body);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // POST /batch/retry
  routes.set('POST /batch/retry', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as BatchRetryRequest;
    const result = await handleBatchRetry(ctx, body);

    return {
      status: result.success ? 200 : 403,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /batch/status
  routes.set('GET /batch/status', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleBatchStatus(ctx);

    return {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  return routes;
}
"#,
        },
        FileTemplate {
            step: 60,
            path: "src/routes/search.ts",
            purpose: "Search routes",
            content: r#"// Search routes
import { Request, Response, Handler, compose, withLogging, withMetrics, withAuth, withRateLimit } from './middleware';
import { RequestContext } from '../handlers/context';
import {
  handleSearch,
  handleSuggest,
  SearchRequestQuery,
  SuggestRequestQuery,
} from '../handlers/search';

export function createSearchRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // GET /search
  routes.set('GET /search', compose(
    withLogging(),
    withMetrics(),
    withRateLimit(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const query = req.query as unknown as SearchRequestQuery;
    const result = await handleSearch(ctx, query);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /search/suggest
  routes.set('GET /search/suggest', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const query = req.query as unknown as SuggestRequestQuery;
    const result = await handleSuggest(ctx, query);

    return {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  return routes;
}
"#,
        },
        FileTemplate {
            step: 61,
            path: "src/routes/export.ts",
            purpose: "Export routes",
            content: r#"// Export routes
import { Request, Response, Handler, compose, withLogging, withMetrics, withAuth } from './middleware';
import { RequestContext } from '../handlers/context';
import {
  handleExportEvents,
  handleExportAudit,
  handleExportMetrics,
  ExportEventsRequest,
  ExportAuditRequest,
  ExportMetricsRequest,
} from '../handlers/export';

export function createExportRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // POST /export/events
  routes.set('POST /export/events', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as ExportEventsRequest;
    const result = await handleExportEvents(ctx, body);

    const contentType = getContentType(body.format);

    return {
      status: result.success ? 200 : 400,
      headers: {
        'Content-Type': contentType,
        'Content-Disposition': `attachment; filename="events.${body.format}"`,
      },
      body: result.success ? result.data : result,
    };
  }));

  // POST /export/audit
  routes.set('POST /export/audit', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as ExportAuditRequest;
    const result = await handleExportAudit(ctx, body);

    return {
      status: result.success ? 200 : 403,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // POST /export/metrics
  routes.set('POST /export/metrics', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as ExportMetricsRequest;
    const result = await handleExportMetrics(ctx, body);

    return {
      status: result.success ? 200 : 403,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  return routes;
}

function getContentType(format: string): string {
  switch (format) {
    case 'json':
      return 'application/json';
    case 'csv':
      return 'text/csv';
    case 'ndjson':
      return 'application/x-ndjson';
    default:
      return 'application/octet-stream';
  }
}
"#,
        },
        FileTemplate {
            step: 62,
            path: "src/routes/admin.ts",
            purpose: "Admin routes",
            content: r#"// Admin routes
import { Request, Response, Handler, compose, withLogging, withMetrics, withAuth } from './middleware';
import { RequestContext } from '../handlers/context';
import {
  handleGetJobs,
  handleJobAction,
  handleCleanupMetrics,
  handleSystemStatus,
  JobActionRequest,
} from '../handlers/admin';

export function createAdminRoutes(): Map<string, Handler> {
  const routes = new Map<string, Handler>();

  // GET /admin/jobs
  routes.set('GET /admin/jobs', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleGetJobs(ctx);

    return {
      status: result.success ? 200 : 403,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // POST /admin/jobs/:id/action
  routes.set('POST /admin/jobs/:id/action', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const jobId = extractPathParam(req.path, '/admin/jobs/', '/action');
    const body = req.body as { action: 'pause' | 'resume' };

    const request: JobActionRequest = {
      jobId,
      action: body.action,
    };

    const result = await handleJobAction(ctx, request);

    return {
      status: result.success ? 200 : 400,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // POST /admin/cleanup-metrics
  routes.set('POST /admin/cleanup-metrics', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const body = req.body as { retentionDays?: number };
    const result = await handleCleanupMetrics(ctx, body?.retentionDays);

    return {
      status: result.success ? 200 : 403,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  // GET /admin/status
  routes.set('GET /admin/status', compose(
    withLogging(),
    withMetrics(),
    withAuth()
  )(async (ctx: RequestContext, req: Request): Promise<Response> => {
    const result = await handleSystemStatus(ctx);

    return {
      status: result.success ? 200 : 403,
      headers: { 'Content-Type': 'application/json' },
      body: result,
    };
  }));

  return routes;
}

function extractPathParam(path: string, prefix: string, suffix: string = ''): string {
  let result = path.replace(prefix, '');
  if (suffix) {
    result = result.replace(suffix, '');
  }
  return result;
}
"#,
        },
        FileTemplate {
            step: 63,
            path: "src/routes/index.ts",
            purpose: "Route aggregation",
            content: r#"// Route aggregation
import { Handler } from './middleware';
import { createAuthRoutes } from './auth';
import { createEventRoutes } from './events';
import { createUserRoutes } from './users';
import { createWebhookRoutes } from './webhooks';
import { createSubscriptionRoutes } from './subscriptions';
import { createMetricsRoutes } from './metrics';
import { createHealthRoutes } from './health';
import { createBatchRoutes } from './batch';
import { createSearchRoutes } from './search';
import { createExportRoutes } from './export';
import { createAdminRoutes } from './admin';

export function createAllRoutes(): Map<string, Handler> {
  const allRoutes = new Map<string, Handler>();

  // Merge all route handlers
  const routeGroups = [
    createAuthRoutes(),
    createEventRoutes(),
    createUserRoutes(),
    createWebhookRoutes(),
    createSubscriptionRoutes(),
    createMetricsRoutes(),
    createHealthRoutes(),
    createBatchRoutes(),
    createSearchRoutes(),
    createExportRoutes(),
    createAdminRoutes(),
  ];

  for (const routes of routeGroups) {
    for (const [key, handler] of routes) {
      allRoutes.set(key, handler);
    }
  }

  return allRoutes;
}

export { Handler, Request, Response, Middleware } from './middleware';
"#,
        },
        FileTemplate {
            step: 64,
            path: "src/app.ts",
            purpose: "Application setup",
            content: r#"// Application setup
import { createAllRoutes, Handler, Request, Response } from './routes';
import { createContext, RequestContext } from './handlers/context';
import { loadServerConfig, ServerConfig } from './config/server';
import { eventProcessor } from './services/processor';
import { scheduler } from './services/scheduler';
import { logger } from './utils/logger';

export interface App {
  config: ServerConfig;
  routes: Map<string, Handler>;
  handleRequest: (req: Request) => Promise<Response>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
}

export function createApp(): App {
  const config = loadServerConfig();
  const routes = createAllRoutes();

  async function handleRequest(req: Request): Promise<Response> {
    const ctx = createContext({
      path: req.path,
      method: req.method,
      ip: req.ip,
      headers: req.headers,
    });

    // Find matching route
    const routeKey = `${req.method} ${req.path}`;
    const handler = findHandler(routes, routeKey);

    if (!handler) {
      return {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
        body: { error: 'Not found' },
      };
    }

    try {
      return await handler(ctx, req);
    } catch (error) {
      logger.error('Unhandled error', {
        requestId: ctx.requestId,
        error: (error as Error).message,
      });

      return {
        status: 500,
        headers: { 'Content-Type': 'application/json' },
        body: { error: 'Internal server error' },
      };
    }
  }

  async function start(): Promise<void> {
    logger.info('Starting application', {
      port: config.port,
      host: config.host,
    });

    // Start background services
    eventProcessor.start();
    scheduler.start();

    logger.info('Application started successfully');
  }

  async function stop(): Promise<void> {
    logger.info('Stopping application');

    await eventProcessor.stop();
    scheduler.stop();

    logger.info('Application stopped');
  }

  return {
    config,
    routes,
    handleRequest,
    start,
    stop,
  };
}

function findHandler(routes: Map<string, Handler>, routeKey: string): Handler | undefined {
  // Exact match first
  if (routes.has(routeKey)) {
    return routes.get(routeKey);
  }

  // Try pattern matching for :id params
  const [method, path] = routeKey.split(' ');

  for (const [key, handler] of routes) {
    const [keyMethod, keyPath] = key.split(' ');

    if (method !== keyMethod) continue;

    if (matchPath(keyPath, path)) {
      return handler;
    }
  }

  return undefined;
}

function matchPath(pattern: string, path: string): boolean {
  const patternParts = pattern.split('/');
  const pathParts = path.split('/');

  if (patternParts.length !== pathParts.length) {
    return false;
  }

  for (let i = 0; i < patternParts.length; i++) {
    if (patternParts[i].startsWith(':')) {
      continue; // Match any value for params
    }
    if (patternParts[i] !== pathParts[i]) {
      return false;
    }
  }

  return true;
}

export const app = createApp();
"#,
        },
        FileTemplate {
            step: 65,
            path: "src/index.ts",
            purpose: "Application entry point",
            content: r#"// Application entry point
import { app } from './app';
import { logger } from './utils/logger';

async function main(): Promise<void> {
  logger.info('Event API Server starting...');

  // Handle graceful shutdown
  process.on('SIGTERM', async () => {
    logger.info('SIGTERM received, shutting down gracefully');
    await app.stop();
    process.exit(0);
  });

  process.on('SIGINT', async () => {
    logger.info('SIGINT received, shutting down gracefully');
    await app.stop();
    process.exit(0);
  });

  // Handle uncaught errors
  process.on('uncaughtException', (error) => {
    logger.error('Uncaught exception', { error: error.message, stack: error.stack });
    process.exit(1);
  });

  process.on('unhandledRejection', (reason) => {
    logger.error('Unhandled rejection', { reason: String(reason) });
    process.exit(1);
  });

  try {
    await app.start();
    logger.info(`Server running on ${app.config.host}:${app.config.port}`);
  } catch (error) {
    logger.error('Failed to start server', { error: (error as Error).message });
    process.exit(1);
  }
}

main();

// Export for testing
export { app };
export * from './types/common';
export * from './types/events';
export * from './types/users';
export * from './services';
export * from './handlers';
"#,
        },
    ]
}

/// Get the total number of steps
pub fn total_steps() -> usize {
    get_templates().len()
}

/// Get template for a specific step
pub fn get_template(step: usize) -> Option<FileTemplate> {
    get_templates().into_iter().find(|t| t.step == step)
}

/// Get all templates up to and including a specific step
pub fn get_templates_through(step: usize) -> Vec<FileTemplate> {
    get_templates().into_iter().filter(|t| t.step <= step).collect()
}
