class Trip < ApplicationRecord
  self.primary_key = :id

  enum :direction_id, { outbound: 0, inbound: 1 }

  belongs_to :route
  belongs_to :service
  belongs_to :shape, optional: true
  has_many :frequencies, dependent: :destroy
  has_many :stop_times, dependent: :destroy
end
