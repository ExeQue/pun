-- set value lists the parser reads leniently, and a statement the file ends inside
CREATE TABLE `odd_things` (
  `bare` set NOT NULL,
  `empty_list` set() NOT NULL,
  `count` int NOT NULL
);
CREATE TABLE `open_things` (`count` int NOT NULL, `open` set('a',
