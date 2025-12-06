import type {
  StoredEpisode as TemporalStoredEpisode,
  StoredFact as TemporalStoredFact,
  TimelineQuery as TemporalTimelineQuery,
  TimestampInput,
} from '../../core/storage/temporal/temporalStore.js';

type TimelineExecutor = (query: TemporalTimelineQuery) => TemporalStoredFact[];
type TraceExecutor = (factId: number) => TemporalStoredEpisode[];

export class TemporalTimelineBuilder {
  private predicateKey?: string;
  private role: 'subject' | 'object' | undefined;
  private asOfValue?: TimestampInput;
  private betweenRange?: [TimestampInput, TimestampInput];

  constructor(
    private readonly entityId: number,
    private readonly executeTimeline: TimelineExecutor,
    private readonly executeTrace: TraceExecutor,
  ) {}

  predicate(key: string): this {
    this.predicateKey = key;
    return this;
  }

  roleAs(role: 'subject' | 'object'): this {
    this.role = role;
    return this;
  }

  asOf(timestamp: TimestampInput): this {
    this.asOfValue = timestamp;
    this.betweenRange = undefined;
    return this;
  }

  between(start: TimestampInput, end: TimestampInput): this {
    this.betweenRange = [start, end];
    this.asOfValue = undefined;
    return this;
  }

  all(): TemporalStoredFact[] {
    const query: TemporalTimelineQuery = {
      entityId: this.entityId,
    };

    if (this.predicateKey) {
      query.predicateKey = this.predicateKey;
    }
    if (this.role) {
      query.role = this.role;
    }
    if (this.asOfValue) {
      query.asOf = this.asOfValue;
    }
    if (this.betweenRange) {
      query.between = this.betweenRange;
    }

    return this.executeTimeline(query);
  }

  first(): TemporalStoredFact | undefined {
    return this.all()[0];
  }

  trace(factId: number): TemporalStoredEpisode[] {
    return this.executeTrace(factId);
  }
}
