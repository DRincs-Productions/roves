[Exposed=Window]
interface LinkedNode {
  readonly attribute unsigned long id;
  readonly attribute LinkedNode? next;
  LinkedNode? follow(unsigned long steps);
  boolean isSame(LinkedNode other);
  unsigned long idOr(LinkedNode? other, unsigned long fallback);
};
