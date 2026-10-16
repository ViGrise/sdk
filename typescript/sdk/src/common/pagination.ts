/** A page of a list. */
export interface Page<T> {
  data: T[];
  /** Pass as `cursor` to read the next page; undefined on the last page. */
  nextCursor?: string;
}
