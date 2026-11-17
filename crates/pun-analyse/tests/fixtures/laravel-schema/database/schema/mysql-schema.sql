/*!40101 SET @OLD_CHARACTER_SET_CLIENT=@@CHARACTER_SET_CLIENT */;
/*!40101 SET NAMES utf8mb4 */;
-- the types and options MySqlDataTypeToPhpTypeConverter reads
DROP TABLE IF EXISTS `widgets`;
CREATE TABLE IF NOT EXISTS `widgets` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `tiny` tinyint(1) NOT NULL DEFAULT '0',
  `small` smallint(5) unsigned zerofill NOT NULL,
  `medium` mediumint NULL,
  `whole` integer NOT NULL DEFAULT -1,
  `real_value` real(8,2) NOT NULL,
  `dbl` double precision unsigned NOT NULL,
  `flt` float NOT NULL DEFAULT .5,
  `dec_value` decimal(10) NOT NULL,
  `num` numeric(8,2) NOT NULL DEFAULT 1.25,
  `fixed_value` fixed NOT NULL,
  `bits` bit(1) NOT NULL,
  `bin` binary(16) NOT NULL,
  `varbin` varbinary(255),
  `code` char(2) binary CHARACTER SET latin1 COLLATE latin1_bin NOT NULL,
  `name` varchar(255) COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT 'it''s \"x\"',
  `alias` character varying(10) NOT NULL,
  `body` longtext binary CHARACTER SET utf8mb4 COLLATE utf8mb4_bin,
  `summary` tinytext NOT NULL DEFAULT "a\nb\tc\rd\e",
  `notes` mediumtext null,
  `plain` text NOT NULL DEFAULT 'NULL',
  `status` enum('draft','it''s') NOT NULL,
  `level` enum('a' 'b') NOT NULL,
  `sizes` set('s','m') CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
  `born_on` date NOT NULL,
  `year_value` year NOT NULL,
  `starts_at` time(3) NOT NULL,
  `seen_at` timestamp NULL DEFAULT NULL,
  `happened_at` datetime NOT NULL,
  `payload` json,
  `geo` point NOT NULL,
  `flag` bool NOT NULL,
  `active` boolean NOT NULL,
  `data` blob NOT NULL,
  `small_data` tinyblob,
  `medium_data` mediumblob,
  `long_data` longblob,
  PRIMARY KEY (`id`),
  UNIQUE KEY `widgets_name_unique` (`name`),
  UNIQUE INDEX `widgets_code_unique` (`code`),
  UNIQUE (`alias`),
  KEY `widgets_tiny_index` (`tiny`),
  INDEX `widgets_small_index` (`small`),
  FULLTEXT KEY `widgets_body_fulltext` (`body`),
  FULLTEXT INDEX `widgets_notes_fulltext` (`notes`),
  FULLTEXT (`plain`),
  SPATIAL INDEX `widgets_geo_spatial` (`geo`),
  SPATIAL KEY `widgets_geo_spatial2` (`geo`),
  SPATIAL (`geo`),
  CONSTRAINT PRIMARY KEY (`id`),
  CONSTRAINT `widgets_code` UNIQUE KEY (`code`),
  CONSTRAINT `widgets_whole_foreign` FOREIGN KEY (`whole`) REFERENCES `others` (`id`) ON DELETE CASCADE,
  FOREIGN KEY (`medium`) REFERENCES `others` (`id`),
  CHECK (`whole` + 1 > 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE TABLE `gadgets` (
  `id` bigint unsigned NOT NULL AUTO_INCREMENT,
  `name` varchar(255) NOT NULL
);
CREATE TABLE `parts` (
  `legacy_code` varchar(20) NOT NULL
);
CREATE TABLE `app`.`qualified_things` (`count` int NOT NULL);
CREATE TEMPORARY TABLE `temp_things` (`count` int NOT NULL);
create table lower_things (count int not null);
CREATE TABLE `dup_things` (`first` int NOT NULL);
CREATE TABLE `dup_things` (`second` int NOT NULL);
CREATE TABLE `likes` LIKE `widgets`;
CREATE TABLE `empty_things`;
CREATE TABLE;
CREATE TABLE `app`.;
CREATE INDEX `widgets_flag_index` ON `widgets` (`flag`);
INSERT INTO `widgets` (`name`) VALUES ('a;b');
