from dataclasses import dataclass
from typing import Generic, List, Optional, TypeVar

T = TypeVar("T")


@dataclass(frozen=True)
class Page(Generic[T]):
  data: List[T]
  # Pass as `cursor` to read the next page; None on the last page.
  next_cursor: Optional[str] = None
