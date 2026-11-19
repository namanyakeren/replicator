# Dashboard saja; agent Windows dan database MySQL berjalan di luar container.
FROM php:8.2-apache

RUN docker-php-ext-install pdo_mysql \
    && printf 'ServerName localhost\n' > /etc/apache2/conf-available/replicator.conf \
    && a2enconf replicator \
    && mkdir -p /var/www/config \
    && chown www-data:www-data /var/www/config

# Document root image Apache adalah /var/www/html, bukan root repositori.
COPY --chown=www-data:www-data dashboard/ /var/www/html/

ENV REPLICATOR_DASHBOARD_CONFIG=/var/www/config/dashboard.ini

EXPOSE 80
